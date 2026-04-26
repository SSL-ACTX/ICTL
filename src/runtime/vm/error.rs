use crate::runtime::memory::MemoryError;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum TemporalError {
    #[error("Temporal fault: branch budget exceeded")]
    BudgetExhausted,
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
    #[error("Missing capability handler for: {0}")]
    MissingCapability(String),
    #[error("Type mismatch: {0}")]
    TypeMismatch(String),
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
    #[error("Causal Paradox: Attempted to rewind past an irreversible global effect (e.g., consumed channel send)")]
    Paradox,
    #[error("Speculation collapsed or failed")]
    SpeculationCollapsed,
}
