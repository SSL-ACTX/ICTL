// src/runtime/vm.rs
use crate::frontend::ast::{
    Capability, EntropyMode, Expression, MergeResolution, ResolutionStrategy,
    Statement, TimeCoordinate,
};
use crate::runtime::gc::GarbageCollector;
use crate::runtime::memory::{Arena, EntropicState, MemoryError, Payload};
use std::collections::{HashMap, VecDeque};
use thiserror::Error;

type CapHandler = Box<dyn Fn(&HashMap<String, String>) -> Result<(), String>>;

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
    #[error("Watchdog bite: Branch '{0}' exceeded {1}ms limit")]
    WatchdogBite(String, u64),
}

pub struct Vm {
    pub global_clock: u64,
    pub root_timeline: Timeline,
    pub active_branches: HashMap<String, Timeline>,
    pub capability_handlers: HashMap<String, CapHandler>,
    pub channels: HashMap<String, VecDeque<Payload>>,
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
}

#[derive(Clone)]
#[allow(dead_code)]
pub struct AnchorPoint {
    pub name: String,
    pub clock_snapshot: u64,
    pub arena_snapshot: Arena,
}

impl Vm {
    pub fn new() -> Self {
        Self {
            global_clock: 0,
            root_timeline: Timeline::new("main".to_string(), 1024 * 1024, 0),
            active_branches: HashMap::new(),
            capability_handlers: HashMap::new(),
            channels: HashMap::new(),
        }
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
                for inner_stmt in body {
                    self.execute_statement(branch_id, inner_stmt)?;
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
                    if let Some(_existing) = merged_data.get::<String>(key) {
                        let strategy =
                            resolution.rules.get(key).ok_or_else(|| {
                                TemporalError::UnresolvedCollision(key.clone())
                            })?;
                        match strategy {
                            ResolutionStrategy::FirstWins => { /* Keep */ }
                            ResolutionStrategy::Priority(p) => {
                                if branch_name == p {
                                    merged_data.insert(
                                        key.clone(),
                                        EntropicState::Valid(payload.clone()),
                                    );
                                }
                            }
                            _ => {
                                return Err(TemporalError::EvalError(
                                    "Strategy not implemented".to_string(),
                                ))
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
