// src/analysis/analyzer.rs
use crate::frontend::ast::*;
use std::collections::{HashMap, HashSet};
use thiserror::Error;

#[allow(dead_code)]
#[derive(Debug, Error)]
pub enum SemanticErrorKind {
    #[error("Compile-Time Entropic Violation: '{0}' has been consumed or decayed and cannot be moved/reused.")]
    UseAfterConsume(String),
    #[error("Merge Collision: Variable '{0}' produced in multiple branches requires a resolution strategy.")]
    UnresolvedMerge(String),
    #[error("Branch Leak: Variable '{0}' is consumed in one branch but accessed in a parallel timeline.")]
    CrossBranchViolation(String),
    #[error("Entropy Mismatch: variables require reconcile: {0}")]
    EntropyMismatch(String),
    #[error("Invalid 'loop' budget: max must be >0")]
    InvalidLoopBudget,
    #[error("Pacing violation: loop body exceeds pacing window")]
    PacingViolation,
    #[error("Invalid Access: '{0}' is not a structure or has decayed.")]
    InvalidStructuralAccess(String),
    #[error("Capability violation: Required capability '{0}' is not declared in this isolate.")]
    MissingCapability(String),
}

#[derive(Debug)]
pub struct SemanticError {
    pub kind: SemanticErrorKind,
    pub branch: String,
    pub statement: Option<String>,
    pub file: Option<String>,
    pub line: Option<usize>,
    pub column: Option<usize>,
}

impl std::fmt::Display for SemanticError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let location_prefix = match (&self.file, self.line, self.column) {
            (Some(file), Some(line), Some(col)) => {
                format!("{}:{}:{}", file, line, col)
            }
            _ => "<unknown>".to_string(),
        };

        write!(f, "error: {}\n --> {}\n  |\n", self.kind, location_prefix)?;

        if let Some(ref stmt) = self.statement {
            write!(f, "  | {}\n", stmt)?;
        }

        write!(f, "  |\n  = note: branch '{}'\n", self.branch)?;

        Ok(())
    }
}

impl std::error::Error for SemanticError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        None
    }
}

#[derive(Clone, Default)]
struct BranchState {
    consumed: HashSet<String>,
    yields: HashSet<String>, // Variables produced or re-assigned in this branch
}

pub struct EntropicAnalyzer {
    // Tracks the entropic state of every active timeline branch
    branch_contexts: HashMap<String, BranchState>,
    current_branch: String,
    current_statement: Option<String>,
    current_span: Option<crate::frontend::ast::Span>,
    source: Option<String>,
    filename: Option<String>,
    capability_stack: Vec<HashSet<String>>,
}

impl EntropicAnalyzer {
    pub fn new() -> Self {
        let mut contexts = HashMap::new();
        contexts.insert("main".to_string(), BranchState::default());
        Self {
            branch_contexts: contexts,
            current_branch: "main".to_string(),
            current_statement: None,
            current_span: None,
            source: None,
            filename: None,
            capability_stack: Vec::new(),
        }
    }

    pub fn analyze_program_with_source(
        &mut self,
        program: &Program,
        source: &str,
        filename: &str,
    ) -> Result<(), SemanticError> {
        self.source = Some(source.to_string());
        self.filename = Some(filename.to_string());
        let result = self.analyze_program(program);
        self.source = None;
        self.filename = None;
        result
    }

    fn annotate(&self, kind: SemanticErrorKind) -> SemanticError {
        let (line, column) =
            if let (Some(span), Some(src)) = (&self.current_span, &self.source) {
                let before = &src[..span.start];
                let ln = before.lines().count() + 1;
                let col = before
                    .lines()
                    .last()
                    .map(|line| line.len() + 1)
                    .unwrap_or(1);
                (Some(ln), Some(col))
            } else {
                (None, None)
            };

        SemanticError {
            kind,
            branch: self.current_branch.clone(),
            statement: self.current_statement.clone(),
            file: self.filename.clone(),
            line,
            column,
        }
    }

    fn is_capability_allowed(&self, cap: &str) -> bool {
        self.capability_stack
            .iter()
            .rev()
            .any(|set| set.contains(cap))
    }

    pub fn analyze_program(
        &mut self,
        program: &Program,
    ) -> Result<(), SemanticError> {
        for block in &program.timelines {
            // Root-level blocks typically start in the context specified by the timeline block
            let old_branch = self.current_branch.clone();
            if let TimeCoordinate::Branch(id) = &block.time {
                self.current_branch = id.clone();
            }

            for stmt in &block.statements {
                let old_stmt = self.current_statement.clone();
                let old_span = self.current_span.clone();
                self.current_statement = Some(self.statement_snippet(stmt));
                self.current_span = Some(stmt.span.clone());
                self.analyze_statement(stmt)?;
                self.current_statement = old_stmt;
                self.current_span = old_span;
            }

            self.current_branch = old_branch;
        }
        Ok(())
    }

    fn analyze_statement(
        &mut self,
        stmt: &crate::frontend::ast::SpannedStatement,
    ) -> Result<(), SemanticError> {
        match &stmt.stmt {
            Statement::RelativisticBlock { time, body } => {
                let old_branch = self.current_branch.clone();
                // If the block specifies a branch, shift the analyzer's focus
                if let TimeCoordinate::Branch(id) = time {
                    self.current_branch = id.clone();
                }

                for inner_stmt in body {
                    self.analyze_statement(inner_stmt)?;
                }

                self.current_branch = old_branch;
            }
            Statement::Watchdog { recovery, .. } => {
                // Analyze recovery statements in the current monitor branch context
                for inner_stmt in recovery {
                    self.analyze_statement(inner_stmt)?;
                }
            }
            Statement::If {
                condition,
                then_branch,
                else_branch,
                reconcile,
            } => {
                self.analyze_expression(condition)?;

                let original_state = self
                    .branch_contexts
                    .get(&self.current_branch)
                    .cloned()
                    .unwrap_or_default();

                // Simulate then-branch
                let then_state = original_state.clone();
                let mut then_contexts = self.branch_contexts.clone();
                then_contexts
                    .insert(self.current_branch.clone(), then_state.clone());
                let previous_contexts =
                    std::mem::replace(&mut self.branch_contexts, then_contexts);

                for inner_stmt in then_branch {
                    self.analyze_statement(inner_stmt)?;
                }
                let then_end_state = self
                    .branch_contexts
                    .get(&self.current_branch)
                    .cloned()
                    .unwrap_or_default();

                // Simulate else-branch
                self.branch_contexts = previous_contexts.clone();
                let else_state = original_state.clone();
                let mut else_contexts = self.branch_contexts.clone();
                else_contexts
                    .insert(self.current_branch.clone(), else_state.clone());
                self.branch_contexts = else_contexts;

                if let Some(else_branch) = else_branch {
                    for inner_stmt in else_branch {
                        self.analyze_statement(inner_stmt)?;
                    }
                }
                let else_end_state = self
                    .branch_contexts
                    .get(&self.current_branch)
                    .cloned()
                    .unwrap_or_default();

                // Determine entropy mismatches
                let mut mismatch_vars = Vec::new();
                for name in then_end_state
                    .consumed
                    .union(&else_end_state.consumed)
                    .cloned()
                {
                    let in_then = then_end_state.consumed.contains(&name);
                    let in_else = else_end_state.consumed.contains(&name);
                    if in_then != in_else {
                        mismatch_vars.push(name);
                    }
                }

                if !mismatch_vars.is_empty() && reconcile.is_none() {
                    return Err(self.annotate(SemanticErrorKind::EntropyMismatch(
                        mismatch_vars.join(", "),
                    )));
                }

                // Merge contexts conservatively: consumed union and yields union
                let merged_state = BranchState {
                    consumed: then_end_state
                        .consumed
                        .union(&else_end_state.consumed)
                        .cloned()
                        .collect(),
                    yields: then_end_state
                        .yields
                        .union(&else_end_state.yields)
                        .cloned()
                        .collect(),
                };

                self.branch_contexts = previous_contexts;
                self.branch_contexts
                    .insert(self.current_branch.clone(), merged_state);
            }
            Statement::Loop { max_ms, body } => {
                if *max_ms == 0 {
                    return Err(self.annotate(SemanticErrorKind::InvalidLoopBudget));
                }
                for inner_stmt in body {
                    self.analyze_statement(inner_stmt)?;
                }
            }
            Statement::Isolate(block) => {
                let mut cap_set = HashSet::new();
                for cap in &block.manifest.capabilities {
                    cap_set.insert(cap.path.clone());
                }
                self.capability_stack.push(cap_set);

                for inner_stmt in &block.body {
                    self.analyze_statement(inner_stmt)?;
                }

                self.capability_stack.pop();
            }
            Statement::Assignment { target, expr } => {
                self.analyze_expression(expr)?;
                let state =
                    self.branch_contexts.get_mut(&self.current_branch).unwrap();

                // Assigning to a target "revives" it in this timeline
                state.consumed.remove(target);
                state.yields.insert(target.clone());
            }
            Statement::Split { parent, branches } => {
                let parent_state = self
                    .branch_contexts
                    .get(parent)
                    .cloned()
                    .unwrap_or_default();

                for branch in branches {
                    // MITOSIS: Children inherit the 'consumed' history (causal past)
                    // but start with zero 'yields' (local production).
                    self.branch_contexts.insert(
                        branch.clone(),
                        BranchState {
                            consumed: parent_state.consumed.clone(),
                            yields: HashSet::new(),
                        },
                    );
                }
                // The split parent identifier itself is consumed by the split operation
                self.mark_consumed(parent)?;
            }
            Statement::Merge {
                branches,
                target,
                resolutions,
            } => {
                let mut all_yields = HashSet::new();
                let mut collisions = HashSet::new();

                for branch_name in branches {
                    let branch_state =
                        self.branch_contexts.get(branch_name).ok_or_else(|| {
                            self.annotate(SemanticErrorKind::CrossBranchViolation(
                                branch_name.clone(),
                            ))
                        })?;

                    for y in &branch_state.yields {
                        if !all_yields.insert(y.clone()) {
                            collisions.insert(y.clone());
                        }
                    }
                }

                // Verify every collision has an explicit resolution rule
                for key in collisions {
                    if !resolutions.rules.contains_key(&key) {
                        return Err(
                            self.annotate(SemanticErrorKind::UnresolvedMerge(key))
                        );
                    }
                }

                let target_state =
                    self.branch_contexts.entry(target.clone()).or_default();
                for y in all_yields {
                    // Merging yields into the target branch revives them
                    target_state.yields.insert(y.clone());
                    target_state.consumed.remove(&y);
                }
            }
            Statement::Send { value_id, .. } => {
                self.mark_consumed(value_id)?;
            }
            Statement::ChannelSend { value_id, .. } => {
                // Sending to a channel is a destructive move
                self.mark_consumed(value_id)?;
            }
            Statement::Break => {
                // break only affects runtime loop flow, no semantic entropic change
            }
            Statement::Select {
                max_ms: _,
                cases,
                timeout,
                reconcile,
            } => {
                let original_state = self
                    .branch_contexts
                    .get(&self.current_branch)
                    .cloned()
                    .unwrap_or_default();

                let mut branch_results = Vec::new();

                for case in cases {
                    self.analyze_expression(&case.source)?;

                    let saved_contexts = self.branch_contexts.clone();
                    let mut branch_contexts = self.branch_contexts.clone();
                    branch_contexts
                        .insert(self.current_branch.clone(), original_state.clone());
                    self.branch_contexts = branch_contexts;

                    // case binding is local to the select branch and not propagated out

                    for stmt in &case.body {
                        self.analyze_statement(stmt)?;
                    }

                    let mut end_state = self
                        .branch_contexts
                        .get(&self.current_branch)
                        .cloned()
                        .unwrap_or_default();
                    end_state.consumed.remove(&case.binding);
                    end_state.yields.remove(&case.binding);
                    branch_results.push(end_state);
                    self.branch_contexts = saved_contexts;
                }

                if let Some(timeout_body) = timeout {
                    let saved_contexts = self.branch_contexts.clone();
                    let mut branch_contexts = self.branch_contexts.clone();
                    branch_contexts
                        .insert(self.current_branch.clone(), original_state.clone());
                    self.branch_contexts = branch_contexts;

                    for stmt in timeout_body {
                        self.analyze_statement(stmt)?;
                    }

                    let end_state = self
                        .branch_contexts
                        .get(&self.current_branch)
                        .cloned()
                        .unwrap_or_default();
                    branch_results.push(end_state);
                    self.branch_contexts = saved_contexts;
                }

                let merged_state = if branch_results.is_empty() {
                    original_state.clone()
                } else {
                    let mut merged = original_state.clone();
                    for st in &branch_results {
                        merged.consumed.extend(st.consumed.clone().into_iter());
                        merged.yields.extend(st.yields.clone().into_iter());
                    }
                    merged
                };

                // Determine variables that are consumed in some but not all branches (entropy branching mismatch)
                let all_vars: std::collections::HashSet<_> = branch_results
                    .iter()
                    .flat_map(|s| s.consumed.iter().cloned())
                    .collect();

                let mut mismatch_vars = Vec::new();
                for var in all_vars {
                    let in_some =
                        branch_results.iter().any(|s| s.consumed.contains(&var));
                    let in_all =
                        branch_results.iter().all(|s| s.consumed.contains(&var));
                    if in_some && !in_all {
                        mismatch_vars.push(var.clone());
                    }
                }

                if !mismatch_vars.is_empty() && reconcile.is_none() {
                    return Err(self.annotate(SemanticErrorKind::EntropyMismatch(
                        mismatch_vars.join(", "),
                    )));
                }

                if let Some(rule) = reconcile {
                    for var in mismatch_vars {
                        if !rule.rules.contains_key(&var) {
                            return Err(self
                                .annotate(SemanticErrorKind::EntropyMismatch(var)));
                        }
                    }
                }

                self.branch_contexts
                    .insert(self.current_branch.clone(), merged_state);
            }
            Statement::MatchEntropy {
                target: _target,
                valid_branch,
                decayed_branch,
                consumed_branch,
            } => {
                let original_state = self
                    .branch_contexts
                    .get(&self.current_branch)
                    .cloned()
                    .unwrap_or_default();

                let mut context_candidates = Vec::new();

                if let Some((binding, branch_body)) = valid_branch {
                    let saved_contexts = self.branch_contexts.clone();
                    let mut branch_contexts = self.branch_contexts.clone();
                    branch_contexts
                        .insert(self.current_branch.clone(), original_state.clone());
                    self.branch_contexts = branch_contexts;

                    self.branch_contexts
                        .get_mut(&self.current_branch)
                        .unwrap()
                        .yields
                        .insert(binding.clone());

                    for stmt in branch_body {
                        self.analyze_statement(stmt)?;
                    }

                    let end_state = self
                        .branch_contexts
                        .get(&self.current_branch)
                        .cloned()
                        .unwrap_or_default();
                    context_candidates.push(end_state);
                    self.branch_contexts = saved_contexts;
                }

                if let Some((binding, branch_body)) = decayed_branch {
                    let saved_contexts = self.branch_contexts.clone();
                    let mut branch_contexts = self.branch_contexts.clone();
                    branch_contexts
                        .insert(self.current_branch.clone(), original_state.clone());
                    self.branch_contexts = branch_contexts;

                    self.branch_contexts
                        .get_mut(&self.current_branch)
                        .unwrap()
                        .yields
                        .insert(binding.clone());

                    for stmt in branch_body {
                        self.analyze_statement(stmt)?;
                    }

                    let end_state = self
                        .branch_contexts
                        .get(&self.current_branch)
                        .cloned()
                        .unwrap_or_default();
                    context_candidates.push(end_state);
                    self.branch_contexts = saved_contexts;
                }

                if let Some(branch_body) = consumed_branch {
                    let saved_contexts = self.branch_contexts.clone();
                    let mut branch_contexts = self.branch_contexts.clone();
                    branch_contexts
                        .insert(self.current_branch.clone(), original_state.clone());
                    self.branch_contexts = branch_contexts;

                    for stmt in branch_body {
                        self.analyze_statement(stmt)?;
                    }

                    let end_state = self
                        .branch_contexts
                        .get(&self.current_branch)
                        .cloned()
                        .unwrap_or_default();
                    context_candidates.push(end_state);
                    self.branch_contexts = saved_contexts;
                }

                let merged_state = context_candidates.into_iter().fold(
                    original_state.clone(),
                    |mut acc, s| {
                        acc.consumed.extend(s.consumed.into_iter());
                        acc.yields.extend(s.yields.into_iter());
                        acc
                    },
                );

                self.branch_contexts
                    .insert(self.current_branch.clone(), merged_state);
            }
            Statement::SpeculationMode(_) => {
                // language-level mode settings affect runtime configuration only
            }
            Statement::Expression(expr) => {
                self.analyze_expression(expr)?;
            }
            Statement::Commit(body) => {
                for inner_stmt in body {
                    self.analyze_statement(inner_stmt)?;
                }
            }
            Statement::Yield(_) => {
                // yields are handled by SplitMap gather semantics
            }
            Statement::For {
                item_name: _,
                mode,
                source,
                body,
                pacing_ms,
                max_ms,
            } => {
                if let crate::frontend::ast::ForMode::Consume = mode {
                    self.mark_consumed(source)?;
                }

                if let Some(max) = max_ms {
                    if *max == 0 {
                        return Err(
                            self.annotate(SemanticErrorKind::InvalidLoopBudget)
                        );
                    }
                }

                // Analyze body statements for entropic effects and branch costs.
                for inner_stmt in body {
                    self.analyze_statement(inner_stmt)?;
                }

                if let Some(pacing) = pacing_ms {
                    let body_cost = body.len() as u64;
                    if body_cost > *pacing {
                        return Err(
                            self.annotate(SemanticErrorKind::PacingViolation)
                        );
                    }
                }
            }
            Statement::Speculate {
                max_ms: _,
                body,
                fallback,
            } => {
                let context_snapshot = self.branch_contexts.clone();

                for stmt in body {
                    self.analyze_statement(stmt)?;
                }

                self.branch_contexts = context_snapshot.clone();

                if let Some(fallback_body) = fallback {
                    for stmt in fallback_body {
                        self.analyze_statement(stmt)?;
                    }
                }

                self.branch_contexts = context_snapshot;
            }
            Statement::Collapse => {
                // collapse is control flow only, no entropic change here
            }
            Statement::SplitMap {
                item_name: _,
                mode: _,
                source,
                body,
                reconcile: _,
            } => {
                self.mark_consumed(source)?;
                for inner_stmt in body {
                    self.analyze_statement(inner_stmt)?;
                }
            }
            Statement::Anchor(_)
            | Statement::Rewind(_)
            | Statement::ChannelOpen { .. }
            | Statement::NetworkRequest { .. }
            | Statement::AcausalReset { .. } => {
                // These statements have no direct impact on the local arena's entropy
            }
            Statement::Capability(cap) => {
                if !self.is_capability_allowed(&cap.path) {
                    return Err(self.annotate(
                        SemanticErrorKind::MissingCapability(cap.path.clone()),
                    ));
                }
            }
        }
        Ok(())
    }

    fn analyze_expression(
        &mut self,
        expr: &Expression,
    ) -> Result<(), SemanticError> {
        match expr {
            Expression::Identifier(name) => self.mark_consumed(name),
            Expression::FieldAccess { parent, .. } => {
                // Structural Decay: Accessing a field consumes the "wholeness" of the parent.
                self.mark_consumed(parent)
            }
            Expression::CloneOp(name) => {
                let state = self.branch_contexts.get(&self.current_branch).unwrap();
                if state.consumed.contains(name) {
                    return Err(self.annotate(SemanticErrorKind::UseAfterConsume(
                        name.clone(),
                    )));
                }
                Ok(())
            }
            Expression::StructLit(fields) => {
                for (_, inner_expr) in fields {
                    self.analyze_expression(inner_expr)?;
                }
                Ok(())
            }
            Expression::ChannelReceive(_)
            | Expression::Literal(_)
            | Expression::Integer(_)
            | Expression::ArrayLiteral(_) => {
                // Receive creates new state; Literals, integers, and arrays are constant
                Ok(())
            }
            Expression::BinaryOp { left, right, .. } => {
                self.analyze_expression(left)?;
                self.analyze_expression(right)?;
                Ok(())
            }
        }
    }

    fn mark_consumed(&mut self, name: &str) -> Result<(), SemanticError> {
        let state = self.branch_contexts.get_mut(&self.current_branch).unwrap();
        if state.consumed.contains(name) {
            return Err(
                self.annotate(SemanticErrorKind::UseAfterConsume(name.to_string()))
            );
        }
        state.consumed.insert(name.to_string());
        Ok(())
    }

    fn statement_snippet(
        &self,
        stmt: &crate::frontend::ast::SpannedStatement,
    ) -> String {
        match &stmt.stmt {
            Statement::Assignment { target, expr } => {
                format!("let {} = {}", target, self.expr_snippet(expr))
            }
            Statement::Split { parent, branches } => {
                format!("split {} into [{}]", parent, branches.join(","))
            }
            Statement::Merge {
                branches, target, ..
            } => {
                format!("merge [{}] into {}", branches.join(","), target)
            }
            Statement::Anchor(name) => format!("anchor {}", name),
            Statement::Rewind(name) => format!("rewind_to({})", name),
            Statement::Commit(_) => "commit { ... }".to_string(),
            Statement::SpeculationMode(_) => "speculation_mode(...)".to_string(),
            Statement::Send {
                value_id,
                target_branch,
            } => {
                format!("send {} to {}", value_id, target_branch)
            }
            Statement::ChannelOpen { name, capacity } => {
                format!("open_chan {}({})", name, capacity)
            }
            Statement::ChannelSend { chan_id, value_id } => {
                format!("chan_send {}({})", chan_id, value_id)
            }
            Statement::Watchdog {
                target, timeout_ms, ..
            } => {
                format!("watchdog {} timeout {}ms", target, timeout_ms)
            }
            Statement::AcausalReset {
                target,
                anchor_name,
            } => {
                format!("reset {} to {}", target, anchor_name)
            }
            Statement::NetworkRequest { domain } => {
                format!("network_request \"{}\"", domain)
            }
            Statement::Isolate(block) => format!(
                "isolate {} {{ ... }}",
                block.name.clone().unwrap_or_default()
            ),
            Statement::RelativisticBlock { time, .. } => match time {
                TimeCoordinate::Branch(id) => format!("@{}: {{ ... }}", id),
                _ => "relativistic block".to_string(),
            },
            Statement::Capability(cap) => format!("require {}(...)", cap.path),
            Statement::If { condition, .. } => {
                format!("if ({}) {{ ... }}", self.expr_snippet(condition))
            }
            Statement::Loop { max_ms, .. } => {
                format!("loop (max {}ms) {{ ... }}", max_ms)
            }
            Statement::Speculate { max_ms, .. } => {
                format!("speculate (max {}ms) {{ ... }}", max_ms)
            }
            Statement::Collapse => "collapse".to_string(),
            Statement::Break => "break".to_string(),
            _ => format!("{:?}", stmt),
        }
    }

    fn expr_snippet(&self, expr: &Expression) -> String {
        match expr {
            Expression::Literal(v) => format!("\"{}\"", v),
            Expression::Identifier(v) => v.clone(),
            Expression::FieldAccess { parent, field } => {
                format!("{}.{}", parent, field)
            }
            Expression::CloneOp(v) => format!("clone({})", v),
            Expression::StructLit(fields) => {
                let mut parts = Vec::new();
                for (k, v) in fields {
                    parts.push(format!("{} = {}", k, self.expr_snippet(v)));
                }
                format!("struct {{ {} }}", parts.join(", "))
            }
            Expression::ChannelReceive(id) => format!("chan_recv({})", id),
            Expression::ArrayLiteral(elements) => {
                let parts: Vec<String> =
                    elements.iter().map(|e| self.expr_snippet(e)).collect();
                format!("[{}]", parts.join(","))
            }
            Expression::Integer(v) => format!("{}", v),
            Expression::BinaryOp { left, op, right } => {
                let op_str = match op {
                    crate::frontend::ast::BinaryOperator::Add => "+",
                    crate::frontend::ast::BinaryOperator::Sub => "-",
                    crate::frontend::ast::BinaryOperator::Mul => "*",
                    crate::frontend::ast::BinaryOperator::Div => "/",
                    crate::frontend::ast::BinaryOperator::Eq => "==",
                    crate::frontend::ast::BinaryOperator::Neq => "!=",
                    crate::frontend::ast::BinaryOperator::Lt => "<",
                    crate::frontend::ast::BinaryOperator::Gt => ">",
                    crate::frontend::ast::BinaryOperator::Le => "<=",
                    crate::frontend::ast::BinaryOperator::Ge => ">=",
                };
                format!(
                    "({} {} {})",
                    self.expr_snippet(left),
                    op_str,
                    self.expr_snippet(right)
                )
            }
        }
    }
}
