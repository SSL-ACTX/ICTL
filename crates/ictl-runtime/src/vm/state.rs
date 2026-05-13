use ictl_core::value::{Arena, Payload};
use ictl_core::{Manifest, ParamMode, SpeculationCommitMode};
use std::collections::{HashMap, VecDeque};

pub type CapHandler = Box<dyn Fn(&HashMap<String, String>) -> Result<(), String>>;

#[derive(Debug, Clone)]
pub struct Message {
    pub id: u64,
    #[allow(dead_code)]
    pub sender: String,
    pub payload: Payload,
}

#[derive(Debug, Clone)]
pub enum CausalEvent {
    ChannelSend {
        branch_id: String,
        channel_id: String,
        payload_id: u64,
    },
    ChannelRecv {
        branch_id: String,
        channel_id: String,
        message: Message,
    },
    InterBranchMove {
        source_branch: String,
        target_branch: String,
        reg: u32,
        #[allow(dead_code)]
        message: Message,
    },
}

#[derive(Clone)]
#[allow(dead_code)]
pub struct AnchorPoint {
    pub name: String,
    pub clock_snapshot: u64,
    pub arena_snapshot: Arena,
    pub cpu_budget_snapshot: u64,
    pub resource_budgets_snapshot: HashMap<String, u64>,
    pub history_index: usize,
    pub pc_snapshot: usize,
    pub instructions_snapshot: Vec<ictl_frontend::ir::Instruction>,
}

#[derive(Clone)]
pub struct Routine {
    pub params: Vec<(ParamMode, String, ictl_core::types::Type)>,
    #[allow(dead_code)]
    pub return_type: ictl_core::types::Type,
    pub taking_ms: Option<u64>,
    pub instructions: Vec<ictl_frontend::ir::Instruction>,
}

pub struct SpeculationContext {
    pub speculation_start_state: Timeline,
    pub history_start_index: usize,
    pub fallback_target: usize,
    pub commit_vars: std::collections::HashSet<u32>,
    pub in_commit_block: bool,
    pub commit_executed: bool,
    pub collapse_happened: bool,
}

pub struct Vm {
    pub symbols: std::collections::HashMap<String, ictl_frontend::ir::Reg>,
    pub speculative_commit_mode: SpeculationCommitMode,
    pub global_clock: u64,
    pub root_timeline: Timeline,
    pub active_branches: HashMap<String, Timeline>,
    pub capability_handlers: HashMap<String, CapHandler>,
    pub channels: HashMap<String, VecDeque<Message>>,
    pub pending_channels: HashMap<String, VecDeque<Message>>,
    pub routines: HashMap<String, Routine>,
    pub decay_handlers: HashMap<String, Vec<ictl_frontend::ir::Instruction>>,
    pub type_decay_limits: HashMap<String, u64>,
    pub speculation_stack: Vec<SpeculationContext>,
    pub entanglements: Vec<std::collections::HashSet<(String, u32)>>,
    pub causal_history: Vec<CausalEvent>,
    pub next_payload_id: u64,
    pub trace_entropy: bool,
    pub(crate) _is_decaying: bool,
}

#[derive(Clone)]
#[allow(dead_code)]
pub struct Timeline {
    pub id: String,
    pub birth_global_time: u64,
    pub local_clock: u64,
    pub arena: Arena,
    pub cpu_budget_ms: u64,
    pub slice_ms: Option<u64>,
    pub anchors: HashMap<String, AnchorPoint>,
    pub commit_horizon_passed: bool,
    pub manifest_stack: Vec<Manifest>,
    pub resource_budgets: HashMap<String, u64>,
    pub entropy_mode: ictl_core::EntropyMode,
    pub break_requested: bool,
    pub loop_depth: u32,
    pub loop_stack: Vec<(u64, u64)>, // (start_clock, max_ms)
    pub pc: usize,
    pub instructions: Vec<ictl_frontend::ir::Instruction>,
}

impl Timeline {
    /// Drop any anchor snapshots for this timeline when commit horizon is passed.
    pub fn clear_anchor_snapshots(&mut self) {
        self.anchors.clear();
    }

    /// Clear the timeline arena and release all tracked resources.
    pub fn clear_arena(&mut self) {
        self.arena.clear();
    }
}
