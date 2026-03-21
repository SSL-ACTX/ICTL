// src/runtime/vm.rs
use crate::frontend::ast::{
    Capability, EntropyMode, Expression, MergeResolution, ResolutionStrategy,
    SpeculationCommitMode, Statement, TimeCoordinate,
};
use crate::runtime::gc::GarbageCollector;
use crate::runtime::memory::{Arena, EntropicState, MemoryError, Payload};
use std::collections::{HashMap, VecDeque};
use thiserror::Error;

type CapHandler = Box<dyn Fn(&HashMap<String, String>) -> Result<(), String>>;

#[derive(Clone)]
#[allow(dead_code)]
pub struct AnchorPoint {
    pub name: String,
    pub clock_snapshot: u64,
    pub arena_snapshot: Arena,
}

#[derive(Debug, Error)]
pub enum TemporalError {
    #[error("Temporal fault: branch budget exceeded")]
    BudgetExhausted,
    #[error("Merge collision for key '{0}' without explicit resolution strategy")]
    UnresolvedCollision(String),
    #[error(
        "Paradox: Attempted to rewind past a commit horizon or anchor not found"
    )]
    CommitHorizonViolation,
    #[error("Entropy Violation: Cannot rewind in Non-Deterministic (Chaos) mode")]
    RewindDisabledInChaos,
    #[error("Anchor not found: {0}")]
    AnchorNotFound(String),
    #[error("Branch not found: {0}")]
    BranchNotFound(String),
    #[error("Capability violation: {0}")]
    CapabilityViolation(String),
    #[error("Memory fault: {0}")]
    MemoryFault(#[from] MemoryError),
    #[error("Evaluation error: {0}")]
    EvalError(String),
    #[error("Channel fault: {0}")]
    ChannelFault(String),
    #[error("Break statement used outside of loop")]
    InvalidBreak,
    #[error("Watchdog bite: Branch '{0}' exceeded {1}ms limit")]
    WatchdogBite(String, u64),
    #[error("Pacing violation: body cost exceeded pacing")]
    PacingViolation,
    #[error("Invalid loop budget: max must be >0")]
    InvalidLoopBudget,
    #[error("Speculation collapsed or failed")]
    SpeculationCollapsed,
}

#[derive(Default)]
struct SpeculationContext {
    commit_vars: std::collections::HashSet<String>,
    in_commit_block: bool,
    commit_executed: bool,
    collapse_happened: bool,
}

pub struct Vm {
    pub speculative_commit_mode: SpeculationCommitMode,
    pub global_clock: u64,
    pub root_timeline: Timeline,
    pub active_branches: HashMap<String, Timeline>,
    pub capability_handlers: HashMap<String, CapHandler>,
    pub channels: HashMap<String, VecDeque<Payload>>,
    speculation_stack: Vec<SpeculationContext>,
}

#[derive(Clone)]
#[allow(dead_code)]
pub struct Timeline {
    pub id: String,
    pub birth_global_time: u64,
    pub local_clock: u64,
    pub arena: Arena,
    pub cpu_budget_ms: u64,
    pub anchors: HashMap<String, AnchorPoint>,
    pub commit_horizon_passed: bool,
    pub manifest_stack: Vec<crate::frontend::ast::Manifest>,
    pub entropy_mode: EntropyMode,
    pub break_requested: bool,
    pub loop_depth: u32,
}

impl Vm {
    pub fn new() -> Self {
        Self {
            global_clock: 0,
            root_timeline: Timeline::new("main".to_string(), 1024 * 1024, 0),
            active_branches: HashMap::new(),
            capability_handlers: HashMap::new(),
            channels: HashMap::new(),
            speculation_stack: Vec::new(),
            speculative_commit_mode: SpeculationCommitMode::Selective,
        }
    }

    #[allow(dead_code)]
    pub fn set_speculative_commit_mode(&mut self, mode: SpeculationCommitMode) {
        self.speculative_commit_mode = mode;
    }

    #[allow(dead_code)]
    pub fn register_capability<F>(&mut self, path: &str, handler: F)
    where
        F: Fn(&HashMap<String, String>) -> Result<(), String> + 'static,
    {
        self.capability_handlers
            .insert(path.to_string(), Box::new(handler));
    }

    pub fn execute_statement(
        &mut self,
        branch_id: &str,
        stmt: &crate::frontend::ast::SpannedStatement,
    ) -> Result<(), TemporalError> {
        // Deterministic instruction cost
        {
            let branch = self.get_branch_mut(branch_id)?;
            branch.local_clock += 1;
        }

        match &stmt.stmt {
            Statement::RelativisticBlock { time, body } => {
                let target_branch = match time {
                    TimeCoordinate::Branch(id) => id.clone(),
                    _ => branch_id.to_string(),
                };

                for inner_stmt in body {
                    self.execute_statement(&target_branch, inner_stmt)?;
                }
            }
            Statement::Watchdog {
                target,
                timeout_ms,
                recovery,
            } => {
                let should_bite = if let Ok(branch) = self.get_branch_mut(target) {
                    branch.local_clock > *timeout_ms
                } else {
                    false
                };

                if should_bite {
                    // Phase 13: Instead of deleting, we trigger recovery logic.
                    // The recovery logic may use AcausalReset to fix the branch.
                    for recovery_stmt in recovery {
                        self.execute_statement(branch_id, recovery_stmt)?;
                    }
                    return Err(TemporalError::WatchdogBite(
                        target.clone(),
                        *timeout_ms,
                    ));
                }
            }
            Statement::Speculate {
                max_ms,
                body,
                fallback,
            } => {
                let original_branch = self.get_branch_mut(branch_id)?.clone();
                let original_channels = self.channels.clone();
                let fallback_cost = self
                    .estimate_block_cost(fallback.as_ref().unwrap_or(&Vec::new()));

                let mut speculative_error: Option<TemporalError> = None;

                self.speculation_stack.push(SpeculationContext::default());
                self.set_branch_state(branch_id, original_branch.clone());
                self.channels = original_channels.clone();

                for stmt in body {
                    if let Err(err) = self.execute_statement(branch_id, stmt) {
                        speculative_error = Some(err);
                        break;
                    }

                    let current_clock = self.get_branch_mut(branch_id)?.local_clock;
                    if current_clock.saturating_sub(original_branch.local_clock)
                        > *max_ms
                    {
                        speculative_error = Some(TemporalError::WatchdogBite(
                            branch_id.to_string(),
                            *max_ms,
                        ));
                        break;
                    }
                }

                let speculative_branch_snapshot =
                    self.get_branch_mut(branch_id)?.clone();

                let speculation_context = self
                    .speculation_stack
                    .pop()
                    .expect("speculation stack underflow");

                let commit_valid = speculative_error.is_none()
                    && speculation_context.commit_executed
                    && !speculation_context.collapse_happened;

                // Restore base state before applying either commit or fallback
                self.set_branch_state(branch_id, original_branch.clone());
                self.channels = original_channels.clone();

                if commit_valid {
                    match self.speculative_commit_mode {
                        SpeculationCommitMode::Full => {
                            self.set_branch_state(
                                branch_id,
                                speculative_branch_snapshot,
                            );
                        }
                        SpeculationCommitMode::Selective => {
                            let branch = self.get_branch_mut(branch_id)?;
                            for var in speculation_context.commit_vars.iter() {
                                if let Some(payload) =
                                    speculative_branch_snapshot.arena.peek(var)
                                {
                                    branch.arena.insert(
                                        var.clone(),
                                        EntropicState::Valid(payload),
                                    )?;
                                } else {
                                    branch.arena.set_consumed(var)?;
                                }
                            }
                        }
                    }
                    let branch = self.get_branch_mut(branch_id)?;
                    branch.commit_horizon_passed = true;
                } else if let Some(fallback_body) = fallback {
                    for stmt in fallback_body {
                        self.execute_statement(branch_id, stmt)?;
                    }
                }

                let branch = self.get_branch_mut(branch_id)?;
                let target_clock =
                    original_branch.local_clock + 1 + *max_ms + fallback_cost;
                if branch.local_clock < target_clock {
                    let padding = target_clock - branch.local_clock;
                    branch.local_clock += padding;
                    branch.consume_budget(padding)?;
                }
            }
            Statement::Collapse => {
                if let Some(ctx) = self.speculation_stack.last_mut() {
                    ctx.collapse_happened = true;
                }
                return Err(TemporalError::SpeculationCollapsed);
            }
            Statement::Break => {
                let branch = self.get_branch_mut(branch_id)?;
                if branch.loop_depth == 0 {
                    return Err(TemporalError::InvalidBreak);
                }
                branch.break_requested = true;
            }
            Statement::SpeculationMode(mode) => {
                self.speculative_commit_mode = *mode;
            }
            Statement::AcausalReset {
                target,
                anchor_name,
            } => {
                // PHASE 13: Time-Loop Logic
                // We reach into a target branch and reset its state to a previous anchor.
                let anchor = {
                    let target_branch = self.get_branch_mut(target)?;
                    target_branch
                        .anchors
                        .get(anchor_name)
                        .ok_or_else(|| {
                            TemporalError::AnchorNotFound(anchor_name.clone())
                        })?
                        .clone()
                };

                let target_branch = self.get_branch_mut(target)?;
                target_branch.arena = anchor.arena_snapshot;
                target_branch.local_clock = anchor.clock_snapshot;
                target_branch.commit_horizon_passed = false;
            }
            Statement::Isolate(block) => {
                let (capabilities, cpu_req) = {
                    let branch = self.get_branch_mut(branch_id)?;
                    if let Some(limit_bytes) = block.manifest.memory_budget_bytes {
                        branch.arena.capacity = limit_bytes;
                    }
                    if let Some(mode) = block.manifest.mode {
                        branch.entropy_mode = mode;
                    }
                    branch.manifest_stack.push(block.manifest.clone());
                    (
                        block.manifest.capabilities.clone(),
                        block.manifest.cpu_budget_ms,
                    )
                };

                for cap in &capabilities {
                    self.execute_capability(branch_id, cap)?;
                }

                if let Some(cpu) = cpu_req {
                    let branch = self.get_branch_mut(branch_id)?;
                    if cpu > branch.cpu_budget_ms {
                        return Err(TemporalError::BudgetExhausted);
                    }
                    branch.cpu_budget_ms = cpu;
                }

                for inner_stmt in &block.body {
                    self.execute_statement(branch_id, inner_stmt)?;
                }

                let branch = self.get_branch_mut(branch_id)?;
                branch.manifest_stack.pop();
            }
            Statement::Capability(cap) => {
                self.execute_capability(branch_id, cap)?;
            }
            Statement::Assignment { target, expr } => {
                let payload = self.evaluate_expression(branch_id, expr)?;
                if let Some(ctx) = self.speculation_stack.last_mut() {
                    if ctx.in_commit_block {
                        ctx.commit_vars.insert(target.clone());
                    }
                }
                let branch = self.get_branch_mut(branch_id)?;
                branch
                    .arena
                    .insert(target.clone(), EntropicState::Valid(payload))?;
            }
            Statement::Split { parent, branches } => {
                let branches_str: Vec<&str> =
                    branches.iter().map(|s| s.as_str()).collect();
                self.split_timeline(parent, branches_str)?;
            }
            Statement::Merge {
                branches,
                target,
                resolutions,
            } => {
                let branches_str: Vec<&str> =
                    branches.iter().map(|s| s.as_str()).collect();
                self.merge_timelines(branches_str, target, resolutions)?;
            }
            Statement::Anchor(name) => {
                let branch = self.get_branch_mut(branch_id)?;
                let snapshot = AnchorPoint {
                    name: name.clone(),
                    clock_snapshot: branch.local_clock,
                    arena_snapshot: branch.arena.clone(),
                };
                branch.anchors.insert(name.clone(), snapshot);
            }
            Statement::Rewind(name) => {
                let branch = self.get_branch_mut(branch_id)?;
                if branch.entropy_mode == EntropyMode::Chaos {
                    return Err(TemporalError::RewindDisabledInChaos);
                }
                let anchor = branch.anchors.get(name).ok_or_else(|| {
                    if branch.commit_horizon_passed {
                        TemporalError::CommitHorizonViolation
                    } else {
                        TemporalError::AnchorNotFound(name.clone())
                    }
                })?;
                branch.arena = anchor.arena_snapshot.clone();
            }
            Statement::Commit(body) => {
                if let Some(ctx) = self.speculation_stack.last_mut() {
                    ctx.commit_executed = true;
                    ctx.in_commit_block = true;
                }

                for inner_stmt in body {
                    self.execute_statement(branch_id, inner_stmt)?;
                }

                if let Some(ctx) = self.speculation_stack.last_mut() {
                    ctx.in_commit_block = false;
                }

                let branch = self.get_branch_mut(branch_id)?;
                branch.anchors.clear();
                branch.commit_horizon_passed = true;
            }
            Statement::Send {
                value_id,
                target_branch,
            } => {
                let payload = {
                    let branch = self.get_branch_mut(branch_id)?;
                    branch.arena.consume(value_id)?
                };
                let target = self.get_branch_mut(target_branch)?;
                target
                    .arena
                    .insert(value_id.clone(), EntropicState::Valid(payload))?;
            }
            Statement::ChannelOpen { name, capacity } => {
                self.channels
                    .insert(name.clone(), VecDeque::with_capacity(*capacity));
            }
            Statement::ChannelSend { chan_id, value_id } => {
                let payload = {
                    let branch = self.get_branch_mut(branch_id)?;
                    branch.arena.consume(value_id)?
                };
                let chan = self.channels.get_mut(chan_id).ok_or_else(|| {
                    TemporalError::ChannelFault(format!(
                        "Channel not found: {}",
                        chan_id
                    ))
                })?;
                chan.push_back(payload);
            }
            Statement::If {
                condition,
                then_branch,
                else_branch,
                reconcile,
            } => {
                let cond_payload = self.evaluate_expression(branch_id, condition)?;
                let cond_true = matches!(cond_payload, Payload::Integer(v) if v != 0)
                    || matches!(cond_payload, Payload::String(ref s) if s != "" );

                let then_cost = self.estimate_block_cost(then_branch);
                let else_cost = self.estimate_block_cost(
                    else_branch.as_ref().unwrap_or(&Vec::new()),
                );
                let max_cost = then_cost.max(else_cost) + 1; // 1ms overhead

                // clone environment for speculative branch execution
                let original_channels = self.channels.clone();
                let original_branch = self.get_branch_mut(branch_id)?.clone();

                let then_state = self.simulate_branch(branch_id, then_branch)?;
                self.channels = original_channels.clone();
                let else_state = self.simulate_branch(
                    branch_id,
                    else_branch.as_ref().unwrap_or(&Vec::new()),
                )?;
                self.channels = original_channels.clone();

                let mut final_state = if cond_true {
                    then_state.clone()
                } else {
                    else_state.clone()
                };

                if let Some(reconcile_rules) = reconcile {
                    for (var, strat) in &reconcile_rules.rules {
                        match strat {
                            ResolutionStrategy::FirstWins => {
                                if let Some(p) = then_state.arena.peek(var) {
                                    final_state.arena.insert(
                                        var.clone(),
                                        EntropicState::Valid(p),
                                    )?;
                                } else if let Some(p) = else_state.arena.peek(var) {
                                    final_state.arena.insert(
                                        var.clone(),
                                        EntropicState::Valid(p),
                                    )?;
                                } else {
                                    final_state.arena.set_consumed(var)?;
                                }
                            }
                            ResolutionStrategy::Priority(branch_name) => {
                                if branch_name == "if" {
                                    if let Some(p) = then_state.arena.peek(var) {
                                        final_state.arena.insert(
                                            var.clone(),
                                            EntropicState::Valid(p),
                                        )?;
                                    } else {
                                        final_state.arena.set_consumed(var)?;
                                    }
                                } else if branch_name == "else" {
                                    if let Some(p) = else_state.arena.peek(var) {
                                        final_state.arena.insert(
                                            var.clone(),
                                            EntropicState::Valid(p),
                                        )?;
                                    } else {
                                        final_state.arena.set_consumed(var)?;
                                    }
                                }
                            }
                            ResolutionStrategy::Decay => {
                                final_state.arena.set_consumed(var)?;
                            }
                            ResolutionStrategy::Custom(_) => {
                                // Apply first-wins fallback for custom
                                if let Some(p) = then_state.arena.peek(var) {
                                    final_state.arena.insert(
                                        var.clone(),
                                        EntropicState::Valid(p),
                                    )?;
                                }
                            }
                        }
                    }
                }

                self.set_branch_state(branch_id, final_state);

                let branch = self.get_branch_mut(branch_id)?;
                let run_cost = branch.local_clock - original_branch.local_clock;
                if run_cost < max_cost {
                    let padding = max_cost - run_cost;
                    branch.local_clock += padding;
                    branch.consume_budget(padding)?;
                }
            }
            Statement::For {
                item_name,
                mode,
                source,
                body,
                pacing_ms,
                max_ms,
            } => {
                let source_payload = {
                    let branch = self.get_branch_mut(branch_id)?;
                    let source_payload = match mode {
                        crate::frontend::ast::ForMode::Consume => {
                            branch.arena.consume(source)?
                        }
                        crate::frontend::ast::ForMode::Clone => branch
                            .arena
                            .peek(source)
                            .ok_or(MemoryError::AlreadyConsumed)?,
                    };
                    source_payload
                };

                let elements = match source_payload {
                    Payload::Array(vec) => vec,
                    _ => {
                        return Err(TemporalError::EvalError(
                            "for-source must be array".into(),
                        ))
                    }
                };

                let mut elapsed = 0;
                let max_allowed = max_ms.unwrap_or(u64::MAX);

                for item_value in elements.into_iter() {
                    if elapsed >= max_allowed {
                        break;
                    }

                    {
                        let branch = self.get_branch_mut(branch_id)?;
                        branch.arena.insert(
                            item_name.clone(),
                            EntropicState::Valid(item_value),
                        )?;
                    }

                    let iteration_start =
                        self.get_branch_mut(branch_id)?.local_clock;
                    for stmt in body {
                        self.execute_statement(branch_id, stmt)?;
                        if self.get_branch_mut(branch_id)?.break_requested {
                            let branch = self.get_branch_mut(branch_id)?;
                            branch.break_requested = false;
                            break;
                        }
                    }

                    let body_cost = self.get_branch_mut(branch_id)?.local_clock
                        - iteration_start;
                    let paced = pacing_ms.unwrap_or(body_cost);

                    if body_cost > paced {
                        return Err(TemporalError::PacingViolation);
                    }

                    let pad = paced - body_cost;
                    if pad > 0 {
                        let branch = self.get_branch_mut(branch_id)?;
                        branch.local_clock += pad;
                        branch.consume_budget(pad)?;
                    }

                    elapsed += paced;
                }

                if let Some(max) = max_ms {
                    let branch = self.get_branch_mut(branch_id)?;
                    if branch.local_clock < *max {
                        let padding = *max - branch.local_clock;
                        branch.local_clock += padding;
                        branch.consume_budget(padding)?;
                    }
                }
            }
            Statement::SplitMap {
                item_name,
                mode,
                source,
                body,
                reconcile,
            } => {
                let source_payload = {
                    let branch = self.get_branch_mut(branch_id)?;
                    match mode {
                        crate::frontend::ast::ForMode::Consume => {
                            branch.arena.consume(source)?
                        }
                        crate::frontend::ast::ForMode::Clone => branch
                            .arena
                            .peek(source)
                            .ok_or(MemoryError::AlreadyConsumed)?,
                    }
                };
                let elements = match source_payload {
                    Payload::Array(vec) => vec,
                    _ => {
                        return Err(TemporalError::EvalError(
                            "split_map source must be array".into(),
                        ))
                    }
                };

                let mut results: Vec<Payload> = Vec::new();

                for item_value in elements.into_iter() {
                    let child_name = format!("splitmap_{}", results.len());
                    let child_snapshot = self.get_branch_mut(branch_id)?.clone();

                    self.active_branches
                        .insert(child_name.clone(), child_snapshot);
                    {
                        let child_branch = self.get_branch_mut(&child_name)?;
                        child_branch.arena.insert(
                            item_name.clone(),
                            EntropicState::Valid(item_value),
                        )?;
                    }

                    for stmt in body {
                        self.execute_statement(&child_name, stmt)?;
                    }

                    let yielded = self
                        .get_branch_mut(&child_name)?
                        .arena
                        .peek("yielded")
                        .map(|p| p.clone());
                    if let Some(Payload::Array(arr)) = yielded {
                        results.extend(arr);
                    }

                    self.terminate_branch(&child_name)?;
                }

                let branch = self.get_branch_mut(branch_id)?;
                branch.arena.insert(
                    "splitmap_results".into(),
                    EntropicState::Valid(Payload::Array(results)),
                )?;

                if let Some(_resolver) = reconcile {
                    // placeholder: resolver semantics can be finalized later
                }
            }
            Statement::Yield(item) => {
                let branch = self.get_branch_mut(branch_id)?;
                let value = branch.arena.consume(item)?;
                match branch.arena.peek("yielded") {
                    Some(Payload::Array(mut existing)) => {
                        existing.push(value);
                        branch.arena.insert(
                            "yielded".into(),
                            EntropicState::Valid(Payload::Array(existing)),
                        )?;
                    }
                    _ => {
                        branch.arena.insert(
                            "yielded".into(),
                            EntropicState::Valid(Payload::Array(vec![value])),
                        )?;
                    }
                }
            }
            Statement::Loop { max_ms, body } => {
                let branch = self.get_branch_mut(branch_id)?;
                if *max_ms == 0 {
                    return Err(TemporalError::InvalidLoopBudget);
                }
                branch.loop_depth += 1;
                let loop_start = branch.local_clock;
                let mut iterations = 0;

                while self.get_branch_mut(branch_id)?.local_clock - loop_start
                    < *max_ms
                {
                    iterations += 1;
                    if iterations > 1000 {
                        return Err(TemporalError::WatchdogBite(
                            branch_id.to_string(),
                            *max_ms,
                        ));
                    }

                    let iter_start = self.get_branch_mut(branch_id)?.local_clock;
                    for stmt in body {
                        self.execute_statement(branch_id, stmt)?;
                        if self.get_branch_mut(branch_id)?.break_requested {
                            break;
                        }
                    }

                    if self.get_branch_mut(branch_id)?.break_requested {
                        let branch = self.get_branch_mut(branch_id)?;
                        branch.break_requested = false;
                        break;
                    }

                    if self.get_branch_mut(branch_id)?.local_clock == iter_start {
                        break;
                    }
                }

                let branch = self.get_branch_mut(branch_id)?;
                let target_clock = loop_start + *max_ms;
                if branch.local_clock < target_clock {
                    let padding = target_clock - branch.local_clock;
                    branch.local_clock += padding;
                    branch.consume_budget(padding)?;
                }

                if branch.loop_depth > 0 {
                    branch.loop_depth -= 1;
                }
            }
            Statement::Expression(expr) => {
                self.evaluate_expression(branch_id, expr)?;
            }
            Statement::NetworkRequest { .. } => {
                let branch = self.get_branch_mut(branch_id)?;
                branch.local_clock += 5;
                branch.consume_budget(5)?;
            }
        }
        Ok(())
    }

    fn execute_capability(
        &mut self,
        branch_id: &str,
        cap: &Capability,
    ) -> Result<(), TemporalError> {
        if cap.path == "System.Entropy"
            && cap.parameters.get("mode") == Some(&"chaos".to_string())
        {
            let branch = self.get_branch_mut(branch_id)?;
            branch.entropy_mode = EntropyMode::Chaos;
        }

        if let Some(handler) = self.capability_handlers.get(&cap.path) {
            handler(&cap.parameters).map_err(TemporalError::CapabilityViolation)?;
        }
        Ok(())
    }

    pub fn evaluate_expression(
        &mut self,
        branch_id: &str,
        expr: &Expression,
    ) -> Result<Payload, TemporalError> {
        match expr {
            Expression::Literal(val) => Ok(Payload::String(val.clone())),
            Expression::Identifier(name) => {
                let branch = self.get_branch_mut(branch_id)?;
                let val = branch.arena.consume(name)?;
                Ok(val)
            }
            Expression::FieldAccess { parent, field } => {
                let branch = self.get_branch_mut(branch_id)?;
                let val = branch.arena.consume_field(parent, field)?;
                Ok(val)
            }
            Expression::CloneOp(name) => {
                let branch = self.get_branch_mut(branch_id)?;
                let payload = branch
                    .arena
                    .peek(name)
                    .ok_or(MemoryError::AlreadyConsumed)?;
                let cost = branch.arena.calculate_clone_cost(&payload, 1);
                branch.consume_budget(cost)?;
                Ok(payload)
            }
            Expression::StructLit(fields) => {
                let mut evaluated_fields = HashMap::new();
                for (name, inner_expr) in fields {
                    let payload = self.evaluate_expression(branch_id, inner_expr)?;
                    evaluated_fields
                        .insert(name.clone(), EntropicState::Valid(payload));
                }
                Ok(Payload::Struct(evaluated_fields))
            }
            Expression::ArrayLiteral(elements) => {
                let mut values = Vec::new();
                for expr in elements {
                    values.push(self.evaluate_expression(branch_id, expr)?);
                }
                Ok(Payload::Array(values))
            }
            Expression::ChannelReceive(chan_id) => {
                let chan = self.channels.get_mut(chan_id).ok_or_else(|| {
                    TemporalError::ChannelFault(format!(
                        "Channel not found: {}",
                        chan_id
                    ))
                })?;
                let payload = chan.pop_front().ok_or_else(|| {
                    TemporalError::ChannelFault(format!(
                        "Channel empty: {}",
                        chan_id
                    ))
                })?;
                Ok(payload)
            }
            Expression::Integer(v) => Ok(Payload::Integer(*v)),
            Expression::BinaryOp { left, op, right } => {
                let left_value = self.evaluate_expression(branch_id, left)?;
                let right_value = self.evaluate_expression(branch_id, right)?;
                let l = match left_value {
                    Payload::Integer(i) => i,
                    Payload::String(ref s) => s.parse::<i64>().unwrap_or(0),
                    _ => 0,
                };
                let r = match right_value {
                    Payload::Integer(i) => i,
                    Payload::String(ref s) => s.parse::<i64>().unwrap_or(0),
                    _ => 0,
                };
                let result = match op {
                    crate::frontend::ast::BinaryOperator::Add => l + r,
                    crate::frontend::ast::BinaryOperator::Sub => l - r,
                    crate::frontend::ast::BinaryOperator::Mul => l * r,
                    crate::frontend::ast::BinaryOperator::Div => {
                        if r == 0 {
                            return Err(TemporalError::EvalError(
                                "Division by zero".into(),
                            ));
                        }
                        l / r
                    }
                    crate::frontend::ast::BinaryOperator::Eq => (l == r) as i64,
                    crate::frontend::ast::BinaryOperator::Neq => (l != r) as i64,
                    crate::frontend::ast::BinaryOperator::Lt => (l < r) as i64,
                    crate::frontend::ast::BinaryOperator::Gt => (l > r) as i64,
                    crate::frontend::ast::BinaryOperator::Le => (l <= r) as i64,
                    crate::frontend::ast::BinaryOperator::Ge => (l >= r) as i64,
                };
                Ok(Payload::Integer(result))
            }
        }
    }

    pub fn split_timeline(
        &mut self,
        parent_id: &str,
        branches: Vec<&str>,
    ) -> Result<(), TemporalError> {
        let (base_arena, cpu_budget_ms, entropy_mode) = {
            let parent_timeline = if parent_id == "main" {
                &self.root_timeline
            } else {
                self.active_branches.get(parent_id).ok_or_else(|| {
                    TemporalError::BranchNotFound(parent_id.to_string())
                })?
            };
            (
                parent_timeline.arena.clone(),
                parent_timeline.cpu_budget_ms,
                parent_timeline.entropy_mode,
            )
        };

        for branch_name in branches {
            let new_branch = Timeline {
                id: branch_name.to_string(),
                birth_global_time: self.global_clock,
                local_clock: 0,
                arena: base_arena.clone(),
                cpu_budget_ms,
                anchors: HashMap::new(),
                commit_horizon_passed: false,
                manifest_stack: Vec::new(),
                entropy_mode,
                break_requested: false,
                loop_depth: 0,
            };
            self.active_branches
                .insert(branch_name.to_string(), new_branch);
        }
        Ok(())
    }

    pub fn merge_timelines(
        &mut self,
        branches: Vec<&str>,
        target: &str,
        resolution: &MergeResolution,
    ) -> Result<(), TemporalError> {
        let mut merged_data: HashMap<String, EntropicState> = HashMap::new();
        for branch_name in &branches {
            let branch =
                self.active_branches.get(*branch_name).ok_or_else(|| {
                    TemporalError::BranchNotFound(branch_name.to_string())
                })?;
            for (key, state) in &branch.arena.bindings {
                if let EntropicState::Valid(payload) = state {
                    if let Some(_existing) = merged_data.get(key) {
                        let strategy =
                            resolution.rules.get(key).ok_or_else(|| {
                                TemporalError::UnresolvedCollision(key.clone())
                            })?;
                        eprintln!(
                            "merge key={} existing=true branch={} strategy={:?}",
                            key, branch_name, strategy
                        );
                        match strategy {
                            ResolutionStrategy::FirstWins => { /* Keep */ }
                            ResolutionStrategy::Priority(p) => {
                                eprintln!(
                                    "priority p={} current={}",
                                    p, branch_name
                                );
                                if branch_name == p {
                                    merged_data.insert(
                                        key.clone(),
                                        EntropicState::Valid(payload.clone()),
                                    );
                                }
                            }
                            ResolutionStrategy::Decay => {
                                merged_data
                                    .insert(key.clone(), EntropicState::Consumed);
                            }
                            ResolutionStrategy::Custom(_) => {
                                // Fallback to first-wins on custom resolver.
                            }
                        }
                    } else {
                        merged_data.insert(
                            key.clone(),
                            EntropicState::Valid(payload.clone()),
                        );
                    }
                }
            }
        }
        let target_branch = self.get_branch_mut(target)?;
        for (k, v) in merged_data {
            target_branch.arena.insert(k, v)?;
        }
        for b in branches {
            if let Some(branch) = self.active_branches.remove(b) {
                GarbageCollector::collect_branch(branch);
            }
        }
        Ok(())
    }

    #[allow(dead_code)]
    pub fn terminate_branch(
        &mut self,
        branch_id: &str,
    ) -> Result<(), TemporalError> {
        if branch_id == "main" {
            return Err(TemporalError::BranchNotFound(branch_id.to_string()));
        }

        if self.active_branches.contains_key(branch_id) {
            GarbageCollector::collect_branch_by_id(self, branch_id);
            Ok(())
        } else {
            Err(TemporalError::BranchNotFound(branch_id.to_string()))
        }
    }

    fn get_branch_mut(&mut self, id: &str) -> Result<&mut Timeline, TemporalError> {
        if id == "main" {
            Ok(&mut self.root_timeline)
        } else {
            self.active_branches
                .get_mut(id)
                .ok_or_else(|| TemporalError::BranchNotFound(id.to_string()))
        }
    }

    fn set_branch_state(&mut self, id: &str, state: Timeline) {
        if id == "main" {
            self.root_timeline = state;
        } else {
            self.active_branches.insert(id.to_string(), state);
        }
    }

    fn simulate_branch(
        &mut self,
        branch_id: &str,
        statements: &[crate::frontend::ast::SpannedStatement],
    ) -> Result<Timeline, TemporalError> {
        let original_state = self.get_branch_mut(branch_id)?.clone();
        self.set_branch_state(branch_id, original_state.clone());

        for stmt in statements {
            self.execute_statement(branch_id, stmt)?;
            if self.get_branch_mut(branch_id)?.break_requested {
                break;
            }
        }

        let result = self.get_branch_mut(branch_id)?.clone();
        self.set_branch_state(branch_id, original_state);
        Ok(result)
    }

    fn estimate_block_cost(
        &self,
        block: &Vec<crate::frontend::ast::SpannedStatement>,
    ) -> u64 {
        block
            .iter()
            .map(|stmt| self.estimate_statement_cost(&stmt.stmt))
            .sum()
    }

    fn estimate_statement_cost(
        &self,
        stmt: &crate::frontend::ast::Statement,
    ) -> u64 {
        use crate::frontend::ast::Statement;

        let base = 1;
        let extra = match stmt {
            Statement::NetworkRequest { .. } => 5,
            Statement::Split { .. }
            | Statement::Merge { .. }
            | Statement::Anchor(_)
            | Statement::Rewind(_)
            | Statement::Commit(_)
            | Statement::Send { .. }
            | Statement::ChannelOpen { .. }
            | Statement::ChannelSend { .. }
            | Statement::AcausalReset { .. }
            | Statement::Capability(_)
            | Statement::Assignment { .. }
            | Statement::Expression(_) => 0,
            Statement::RelativisticBlock { body, .. } => {
                self.estimate_block_cost(body)
            }
            Statement::Isolate(block) => self.estimate_block_cost(&block.body),
            Statement::Watchdog { recovery, .. } => {
                self.estimate_block_cost(recovery)
            }
            Statement::If {
                then_branch,
                else_branch,
                ..
            } => {
                1 + self.estimate_block_cost(then_branch).max(
                    self.estimate_block_cost(
                        else_branch.as_ref().unwrap_or(&Vec::new()),
                    ),
                )
            }
            Statement::For { pacing_ms, .. } => {
                let pacing = pacing_ms.unwrap_or(1);
                pacing
            }
            Statement::Speculate { body, fallback, .. } => {
                let fallback_cost = self
                    .estimate_block_cost(fallback.as_ref().unwrap_or(&Vec::new()));
                let body_cost = self.estimate_block_cost(body);
                1 + body_cost + fallback_cost
            }
            Statement::Collapse => 0,
            Statement::SplitMap { .. } => 1,
            Statement::Yield(_) => 0,
            Statement::Loop { max_ms, .. } => *max_ms,
            Statement::SpeculationMode(_) => 0,
            Statement::Break => 0,
        };
        base + extra
    }
}

impl Timeline {
    pub fn new(id: String, memory_capacity: u64, birth_time: u64) -> Self {
        Self {
            id,
            birth_global_time: birth_time,
            local_clock: 0,
            arena: Arena::new(memory_capacity),
            cpu_budget_ms: u64::MAX,
            anchors: HashMap::new(),
            commit_horizon_passed: false,
            manifest_stack: Vec::new(),
            entropy_mode: EntropyMode::Deterministic,
            break_requested: false,
            loop_depth: 0,
        }
    }

    pub fn consume_budget(&mut self, amount: u64) -> Result<(), TemporalError> {
        if self.cpu_budget_ms < amount {
            return Err(TemporalError::BudgetExhausted);
        }
        self.cpu_budget_ms -= amount;
        Ok(())
    }
}
