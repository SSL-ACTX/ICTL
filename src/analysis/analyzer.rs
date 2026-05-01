// src/analysis/analyzer.rs
use crate::analysis::types::{StructType, Type};
use crate::frontend::ast::*;
use std::collections::{HashMap, HashSet};
use thiserror::Error;

use crate::analysis::statement;

#[allow(dead_code)]
#[derive(Debug, Error)]
pub enum SemanticErrorKind {
    #[error("Compile-Time Entropic Violation: '{0}' has been consumed or decayed and cannot be moved/reused.")]
    UseAfterConsume(String),
    #[error("Entropy Violation: Variable '{0}' has decayed after {1}ms (instantiated at {2}ms, currently at {3}ms)")]
    UsedDecayedValue(String, u64, u64, u64),
    #[error("Timeline Violation: Variable '{0}' is scoped to branch '@{1}' and cannot be moved to branch '@{2}'.")]
    InvalidTimelineMove(String, String, String),
    #[error("Merge Collision: Variable '{0}' produced in multiple branches requires a resolution strategy.")]
    UnresolvedMerge(String),
    #[error("Branch Leak: Variable '{0}' is consumed in one branch but accessed in a parallel timeline.")]
    CrossBranchViolation(String),
    #[error("Entropy Mismatch: variables require reconcile: {0}")]
    EntropyMismatch(String),
    #[error("Invalid 'loop' budget: max must be >0")]
    InvalidLoopBudget,
    #[error("Tick loop requires a fixed slice via slice <N>ms")]
    TickLoopWithoutSlice,
    #[error("Type mismatch: {0}")]
    TypeMismatch(String),
    #[error("Undefined variable: {0}")]
    UndefinedVariable(String),
    #[error("Tick loop body cost {0}ms exceeds slice budget {1}ms")]
    TickLoopBudgetExceeded(u64, u64),
    #[error("Tick loop must include a break statement")]
    TickLoopNeedsBreak,
    #[error("Routine temporal contract violated: {0} requires {1}ms but body costs {2}ms")]
    RoutineBudgetExceeded(String, u64, u64),
    #[error("Pacing violation: loop body exceeds pacing window")]
    PacingViolation,
    #[error("Invalid Access: '{0}' is not a structure or has decayed.")]
    InvalidStructuralAccess(String),
    #[error("Capability violation: Required capability '{0}' is not declared in this isolate.")]
    MissingCapability(String),
    #[error("Temporal Assertion Violation: WCET to this point is {0}ms, which exceeds the limit of {1}ms")]
    TemporalAssertionViolation(u64, u64),
    #[error("Chaos Mode enabled: Rewinds and anchors are disabled because non-deterministic entropy was requested.")]
    ChaosModePreventsRewind,
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

        write!(f, "error: {}\n  --> {}\n   |\n", self.kind, location_prefix)?;

        if let Some(ref stmt) = self.statement {
            write!(f, "{:>4} | {}\n", self.line.unwrap_or(0), stmt)?;
            if let Some(col) = self.column {
                let marker_line = " ".repeat(col.saturating_sub(1));
                write!(f, "   | {}^\n", marker_line)?;
            }
        }

        write!(f, "   |\n   = note: branch '{}'\n", self.branch)
    }
}

impl std::error::Error for SemanticError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        None
    }
}

#[derive(Clone, Default)]
pub struct BranchState {
    pub consumed: HashSet<String>,
    pub decayed: HashSet<String>,
    pub yields: HashSet<String>,
    pub mutables: HashSet<String>,
    pub types: HashMap<String, Type>,
    pub custom_types: HashMap<String, Type>,
    pub accumulated_cost: u64,
    pub instantiated_at: HashMap<String, u64>,
}

#[derive(Clone, Debug)]
pub struct RoutineInfo {
    pub params: Vec<(crate::frontend::ast::ParamMode, String, Type)>,
    pub return_type: Type,
    pub taking_ms: u64,
}

pub struct EntropicAnalyzer {
    pub branch_contexts: HashMap<String, BranchState>,
    pub current_branch: String,
    pub current_statement: Option<String>,
    pub current_span: Option<crate::frontend::ast::Span>,
    pub(crate) inspection_depth: usize,
    pub(crate) current_slice_ms: Option<u64>,
    pub source: Option<String>,
    pub(crate) filename: Option<String>,
    pub(crate) capability_stack:
        Vec<HashMap<String, crate::frontend::ast::Capability>>,
    pub(crate) routines: HashMap<String, RoutineInfo>,
    pub span_states: HashMap<Span, BranchState>,
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
            inspection_depth: 0,
            current_slice_ms: None,
            source: None,
            filename: None,
            capability_stack: Vec::new(),
            routines: HashMap::new(),
            span_states: HashMap::new(),
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

    pub(crate) fn annotate(&self, kind: SemanticErrorKind) -> SemanticError {
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

    pub(crate) fn is_capability_allowed(&self, cap: &str) -> bool {
        self.capability_stack
            .iter()
            .rev()
            .any(|map| map.contains_key(cap))
    }

    pub(crate) fn get_capability(
        &self,
        cap: &str,
    ) -> Option<&crate::frontend::ast::Capability> {
        self.capability_stack
            .iter()
            .rev()
            .find_map(|map| map.get(cap))
    }

    pub fn analyze_program(
        &mut self,
        program: &Program,
    ) -> Result<(), SemanticError> {
        // Reset mutable analysis state for each run.
        self.branch_contexts.clear();
        self.branch_contexts
            .insert("main".to_string(), BranchState::default());
        self.current_branch = "main".to_string();
        self.current_statement = None;
        self.current_span = None;
        self.inspection_depth = 0;
        self.current_slice_ms = None;
        self.capability_stack.clear();
        self.routines.clear();

        for block in &program.timelines {
            let old_branch = self.current_branch.clone();
            if let TimeCoordinate::Branch(id) = &block.time {
                self.current_branch = id.clone();
            }

            for stmt in &block.statements {
                let old_stmt = self.current_statement.clone();
                let old_span = self.current_span.clone();
                self.current_statement = Some(self.statement_snippet(stmt));
                self.current_span = Some(stmt.span.clone());
                self.current_span = Some(stmt.span.clone());
                self.analyze_statement(stmt)?;
                self.record_state(stmt.span.clone());
                self.current_statement = old_stmt;
                self.current_span = old_span;
            }

            self.current_branch = old_branch;
        }
        Ok(())
    }

    fn analyze_statement(
        &mut self,
        stmt: &SpannedStatement,
    ) -> Result<(), SemanticError> {
        statement::analyze_statement(self, stmt)
    }

    pub(crate) fn mark_consumed(&mut self, name: &str) -> Result<(), SemanticError> {
        let state = self.branch_contexts.get_mut(&self.current_branch).unwrap();
        if state.consumed.contains(name) || state.decayed.contains(name) {
            return Err(
                self.annotate(SemanticErrorKind::UseAfterConsume(name.to_string()))
            );
        }
        state.consumed.insert(name.to_string());
        Ok(())
    }

    pub(crate) fn mark_decayed(&mut self, name: &str) -> Result<(), SemanticError> {
        let state = self.branch_contexts.get_mut(&self.current_branch).unwrap();
        if state.consumed.contains(name) {
            return Err(
                self.annotate(SemanticErrorKind::UseAfterConsume(name.to_string()))
            );
        }
        state.decayed.insert(name.to_string());
        Ok(())
    }

    pub(crate) fn set_variable_type(&mut self, name: &str, vtype: Type) {
        let state = self.branch_contexts.get_mut(&self.current_branch).unwrap();
        state.types.insert(name.to_string(), vtype);
    }

    pub(crate) fn get_variable_type(&self, name: &str) -> Option<Type> {
        self.branch_contexts
            .get(&self.current_branch)
            .and_then(|state| state.types.get(name).cloned())
    }

    pub(crate) fn set_custom_type(&mut self, name: &str, ctype: Type) {
        let state = self.branch_contexts.get_mut(&self.current_branch).unwrap();
        state.custom_types.insert(name.to_string(), ctype);
    }

    pub(crate) fn get_custom_type(&self, name: &str) -> Option<Type> {
        self.branch_contexts
            .get(&self.current_branch)
            .and_then(|state| state.custom_types.get(name).cloned())
    }

    pub(crate) fn resolve_type(&self, typ: &Type) -> Type {
        match typ {
            Type::Custom(name) => self
                .get_custom_type(name)
                .unwrap_or(Type::Custom(name.clone())),
            Type::Struct(s) => {
                let resolved_fields: std::collections::HashMap<String, Type> = s
                    .fields
                    .iter()
                    .map(|(k, v)| (k.clone(), self.resolve_type(v)))
                    .collect();
                Type::Struct(StructType {
                    fields: resolved_fields,
                    decay_after_ms: s.decay_after_ms,
                    scoped_branch: s.scoped_branch.clone(),
                })
            }
            Type::Topology(fields) => {
                let resolved_fields: std::collections::HashMap<String, Type> =
                    fields
                        .iter()
                        .map(|(k, v)| (k.clone(), self.resolve_type(v)))
                        .collect();
                Type::Topology(resolved_fields)
            }
            Type::Array(inner) => Type::Array(Box::new(self.resolve_type(inner))),
            Type::Optional(inner) => {
                Type::Optional(Box::new(self.resolve_type(inner)))
            }
            Type::Union(items) => Type::Union(
                items.into_iter().map(|t| self.resolve_type(&t)).collect(),
            ),
            Type::Function {
                params,
                return_type,
            } => Type::Function {
                params: params.into_iter().map(|p| self.resolve_type(&p)).collect(),
                return_type: Box::new(self.resolve_type(&return_type)),
            },
            _ => typ.clone(),
        }
    }

    #[allow(dead_code)]
    pub(crate) fn format_semantic_error(&self, err: &SemanticError) -> String {
        let mut message = format!("{}", err.kind);
        if let Some(line) = err.line {
            message.push_str(&format!(" at {}:{}", line, err.column.unwrap_or(0)));
        }
        if let Some(stmt) = &err.statement {
            message.push_str(&format!(" in statement: {}", stmt));
        }
        message
    }

    pub(crate) fn types_compatible(&self, expected: &Type, actual: &Type) -> bool {
        let expected = self.resolve_type(expected);
        let actual = self.resolve_type(actual);

        if matches!(expected, Type::Unknown) || matches!(actual, Type::Unknown) {
            return true;
        }

        match (expected, actual) {
            (Type::Integer, Type::Integer)
            | (Type::Bool, Type::Bool)
            | (Type::String, Type::String) => true,
            (Type::Struct(exp_struct), Type::Struct(act_struct)) => {
                if exp_struct.fields.is_empty() {
                    true
                } else {
                    exp_struct.fields == act_struct.fields
                }
            }
            (Type::Topology(exp_fields), Type::Topology(act_fields)) => {
                if exp_fields.is_empty() {
                    true
                } else {
                    exp_fields == act_fields
                }
            }
            (Type::Array(exp_inner), Type::Array(act_inner)) => {
                self.types_compatible(&exp_inner, &act_inner)
            }
            (Type::Optional(exp_inner), Type::Optional(act_inner)) => {
                self.types_compatible(&exp_inner, &act_inner)
            }
            (Type::Optional(exp_inner), act_ty) => {
                // optional can accept inner type (nullable semantics)
                self.types_compatible(&exp_inner, &act_ty)
            }
            (act_ty, Type::Optional(exp_inner)) => {
                self.types_compatible(&act_ty, &exp_inner)
            }
            (Type::Union(exp_types), act_ty) => {
                exp_types.iter().any(|t| self.types_compatible(t, &act_ty))
            }
            (act_ty, Type::Union(exp_types)) => {
                exp_types.iter().any(|t| self.types_compatible(&act_ty, t))
            }
            (
                Type::Function {
                    params: exp_params,
                    return_type: exp_rt,
                },
                Type::Function {
                    params: act_params,
                    return_type: act_rt,
                },
            ) => {
                exp_params.len() == act_params.len()
                    && exp_params
                        .iter()
                        .zip(act_params.iter())
                        .all(|(e, a)| self.types_compatible(e, a))
                    && self.types_compatible(&exp_rt, &act_rt)
            }
            (Type::Custom(exp_name), Type::Custom(act_name)) => exp_name == act_name,
            (Type::Custom(_), _) => false,
            (_, Type::Custom(_)) => false,
            _ => false,
        }
    }

    fn statement_snippet(&self, stmt: &SpannedStatement) -> String {
        match &stmt.stmt {
            Statement::Assignment { target, expr, .. } => {
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
            Statement::Entangle { variables } => {
                format!("entangle({})", variables.join(","))
            }
            _ => format!("{:?}", stmt),
        }
    }

    fn expr_snippet(&self, expr: &Expression) -> String {
        match expr {
            Expression::Literal(v) => format!("\"{}\"", v),
            Expression::Identifier(v) => v.clone(),
            Expression::Null => "null".to_string(),
            Expression::Boolean(b) => b.to_string(),
            Expression::FieldAccess { target, field } => {
                format!("{}.{}", self.expr_snippet(target), field)
            }
            Expression::CloneOp(v) => format!("clone({})", v),
            Expression::StructLit(fields) => {
                let parts: Vec<String> = fields
                    .iter()
                    .map(|(k, v)| format!("{} = {}", k, self.expr_snippet(v)))
                    .collect();
                format!("struct {{ {} }}", parts.join(", "))
            }
            Expression::TopologyLit(fields) => {
                let parts: Vec<String> = fields
                    .iter()
                    .map(|(k, v)| format!("{} = {}", k, self.expr_snippet(v)))
                    .collect();
                format!("topology {{ {} }}", parts.join(", "))
            }
            Expression::IndexAccess { target, index } => {
                format!(
                    "{}[{}]",
                    self.expr_snippet(target),
                    self.expr_snippet(index)
                )
            }
            Expression::ChannelReceive(id) => format!("chan_recv({})", id),
            Expression::ArrayLiteral(elements) => {
                let parts: Vec<String> =
                    elements.iter().map(|e| self.expr_snippet(e)).collect();
                format!("[{}]", parts.join(","))
            }
            Expression::Integer(v) => format!("{}", v),
            Expression::Deferred { capability, .. } => {
                format!("defer {}(...)", capability)
            }
            Expression::Call { routine, args } => {
                let args_str: Vec<String> =
                    args.iter().map(|e| self.expr_snippet(e)).collect();
                format!("call {}({})", routine, args_str.join(", "))
            }
            Expression::BinaryOp { left, op, right } => {
                let op_str = match op {
                    BinaryOperator::Add => "+",
                    BinaryOperator::Sub => "-",
                    BinaryOperator::Mul => "*",
                    BinaryOperator::Div => "/",
                    BinaryOperator::Eq => "==",
                    BinaryOperator::Neq => "!=",
                    BinaryOperator::Lt => "<",
                    BinaryOperator::Gt => ">",
                    BinaryOperator::Le => "<=",
                    BinaryOperator::Ge => ">=",
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

    pub fn record_state(&mut self, span: Span) {
        if let Some(state) = self.branch_contexts.get(&self.current_branch) {
            self.span_states.insert(span, state.clone());
        }
    }
}
