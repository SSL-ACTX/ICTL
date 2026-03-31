// src/ast.rs
use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Program {
    pub timelines: Vec<TimelineBlock>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
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
pub struct ParamDecl {
    pub mode: ParamMode,
    pub name: String,
    pub typ: Option<TypeName>,
}

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
        mutable: bool,
        var_type: Option<TypeName>,
        expr: Expression,
    },
    TypeDecl {
        name: String,
        fields: HashMap<String, TypeName>,
    },
    #[allow(dead_code)]
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
    Select {
        max_ms: u64,
        cases: Vec<SelectCase>,
        timeout: Option<Vec<SpannedStatement>>,
        reconcile: Option<MergeResolution>,
    },
    MatchEntropy {
        target: Expression,
        valid_branch: Option<(String, Vec<SpannedStatement>)>,
        decayed_branch: Option<(String, Vec<SpannedStatement>)>,
        pending_branch: Option<Vec<SpannedStatement>>,
        consumed_branch: Option<Vec<SpannedStatement>>,
    },
    Await(String),
    If {
        condition: Expression,
        then_branch: Vec<SpannedStatement>,
        else_branch: Option<Vec<SpannedStatement>>,
        reconcile: Option<MergeResolution>,
    },
    Break,
    Inspect {
        target: String,
        body: Vec<SpannedStatement>,
    },
    Loop {
        max_ms: u64,
        body: Vec<SpannedStatement>,
    },
    LoopTick {
        body: Vec<SpannedStatement>,
    },
    Slice {
        milliseconds: u64,
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
    Print(Expression),
    Debug(Expression),
    RoutineDef {
        name: String,
        params: Vec<ParamDecl>,
        return_type: Option<TypeName>,
        taking_ms: Option<u64>,
        body: Vec<SpannedStatement>,
    },
    AcausalReset {
        target: String,
        anchor_name: String,
    },
    Entangle {
        variables: Vec<String>,
    },
    FieldUpdate {
        target: Expression,
        field: String,
        value: Expression,
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
    pub auto: bool,
    pub fallback: Option<Vec<SpannedStatement>>,
    pub taking_ms: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CausalReversion {
    pub branch: String,
    pub anchor: String,
}

#[allow(dead_code)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResolutionStrategy {
    FirstWins,
    Priority(String),
    Decay,
    Auto,
    Custom(String),
    TopologyUnion {
        key_rules: HashMap<String, ResolutionStrategy>,
        default: Box<ResolutionStrategy>,
        on_invalid: Option<CausalReversion>,
    },
    TopologyIntersect {
        key_rules: HashMap<String, ResolutionStrategy>,
        default: Box<ResolutionStrategy>,
        on_invalid: Option<CausalReversion>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SelectCase {
    pub binding: String,
    pub source: Expression,
    pub body: Vec<SpannedStatement>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TypeName {
    Builtin(BuiltinType),
    Custom(String),
    Optional(Box<TypeName>),
    Union(Vec<TypeName>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BuiltinType {
    Integer,
    Bool,
    String,
    Struct,
    Topology,
    Array,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ForMode {
    Consume,
    Clone,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParamMode {
    Consume,
    Clone,
    Decay,
    Peek,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Expression {
    Call {
        routine: String,
        args: Vec<Expression>,
    },
    Literal(String),
    Identifier(String),
    FieldAccess {
        target: Box<Expression>,
        field: String,
    },
    CloneOp(String),
    StructLit(HashMap<String, Expression>),
    TopologyLit(HashMap<String, Expression>),
    IndexAccess {
        target: Box<Expression>,
        index: Box<Expression>,
    },
    ArrayLiteral(Vec<Expression>),
    ChannelReceive(String),
    Integer(i64),
    Boolean(bool),
    BinaryOp {
        left: Box<Expression>,
        op: BinaryOperator,
        right: Box<Expression>,
    },
    Deferred {
        capability: String,
        params: HashMap<String, String>,
        deadline_ms: u64,
    },
    Null,
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
