// src/ast.rs
use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Program {
    pub timelines: Vec<TimelineBlock>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Span {
    pub start: usize,
    pub end: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpannedStatement {
    pub stmt: Statement,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TimelineBlock {
    pub time: TimeCoordinate,
    pub statements: Vec<SpannedStatement>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TimeCoordinate {
    Global(u64),
    Relative(u64),
    Branch(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntropyMode {
    Deterministic,
    Chaos,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpeculationCommitMode {
    Selective,
    Full,
}

#[allow(dead_code)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Statement {
    Isolate(IsolateBlock),
    Split {
        parent: String,
        branches: Vec<String>,
    },
    Merge {
        branches: Vec<String>,
        target: String,
        resolutions: MergeResolution,
    },
    Anchor(String),
    Rewind(String),
    Commit(Vec<SpannedStatement>),
    Assignment {
        target: String,
        expr: Expression,
    },
    Send {
        value_id: String,
        target_branch: String,
    },
    Expression(Expression),
    Capability(Capability),
    ChannelOpen {
        name: String,
        capacity: usize,
    },
    ChannelSend {
        chan_id: String,
        value_id: String,
    },
    RelativisticBlock {
        time: TimeCoordinate,
        body: Vec<SpannedStatement>,
    },
    NetworkRequest {
        domain: String,
    },
    // target: the branch to monitor
    // timeout_ms: the limit on the target branch's local_clock
    // recovery: statements to run if the branch is terminated
    Watchdog {
        target: String,
        timeout_ms: u64,
        recovery: Vec<SpannedStatement>,
    },
    Speculate {
        max_ms: u64,
        body: Vec<SpannedStatement>,
        fallback: Option<Vec<SpannedStatement>>,
    },
    Collapse,
    SpeculationMode(SpeculationCommitMode),
    If {
        condition: Expression,
        then_branch: Vec<SpannedStatement>,
        else_branch: Option<Vec<SpannedStatement>>,
        reconcile: Option<MergeResolution>,
    },
    Break,
    Loop {
        max_ms: u64,
        body: Vec<SpannedStatement>,
    },
    For {
        item_name: String,
        mode: ForMode,
        source: String,
        body: Vec<SpannedStatement>,
        pacing_ms: Option<u64>,
        max_ms: Option<u64>,
    },
    SplitMap {
        item_name: String,
        mode: ForMode,
        source: String,
        body: Vec<SpannedStatement>,
        reconcile: Option<MergeResolution>,
    },
    Yield(String),
    AcausalReset {
        target: String,
        anchor_name: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IsolateBlock {
    pub name: Option<String>,
    pub manifest: Manifest,
    pub body: Vec<SpannedStatement>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Manifest {
    pub cpu_budget_ms: Option<u64>,
    pub memory_budget_bytes: Option<u64>,
    pub capabilities: Vec<Capability>,
    pub mode: Option<EntropyMode>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Capability {
    pub path: String,
    pub parameters: HashMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MergeResolution {
    pub rules: HashMap<String, ResolutionStrategy>,
}

#[allow(dead_code)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResolutionStrategy {
    FirstWins,
    Priority(String),
    Decay,
    Custom(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ForMode {
    Consume,
    Clone,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Expression {
    Literal(String),
    Identifier(String),
    FieldAccess {
        parent: String,
        field: String,
    },
    CloneOp(String),
    StructLit(HashMap<String, Expression>),
    ArrayLiteral(Vec<Expression>),
    ChannelReceive(String),
    Integer(i64),
    BinaryOp {
        left: Box<Expression>,
        op: BinaryOperator,
        right: Box<Expression>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinaryOperator {
    Add,
    Sub,
    Mul,
    Div,
    Eq,
    Neq,
    Lt,
    Gt,
    Le,
    Ge,
}
