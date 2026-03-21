use crate::frontend::ast::{Manifest, ParamMode, SpeculationCommitMode};
use crate::runtime::memory::{Arena, Payload};
use std::collections::{HashMap, VecDeque};

pub type CapHandler = Box<dyn Fn(&HashMap<String, String>) -> Result<(), String>>;

#[derive(Clone)]
#[allow(dead_code)]
pub struct AnchorPoint {
    pub name: String,
    pub clock_snapshot: u64,
    pub arena_snapshot: Arena,
}

#[derive(Clone)]
pub struct Routine {
    pub params: Vec<(ParamMode, String)>,
    pub taking_ms: Option<u64>,
    pub body: Vec<crate::frontend::ast::SpannedStatement>,
}

#[derive(Default)]
pub(crate) struct SpeculationContext {
    pub(crate) commit_vars: std::collections::HashSet<String>,
    pub(crate) in_commit_block: bool,
    pub(crate) commit_executed: bool,
    pub(crate) collapse_happened: bool,
}

pub struct Vm {
    pub speculative_commit_mode: SpeculationCommitMode,
    pub global_clock: u64,
    pub root_timeline: Timeline,
    pub active_branches: HashMap<String, Timeline>,
    pub capability_handlers: HashMap<String, CapHandler>,
    pub channels: HashMap<String, VecDeque<Payload>>,
    pub routines: HashMap<String, Routine>,
    pub(crate) speculation_stack: Vec<SpeculationContext>,
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
    pub manifest_stack: Vec<Manifest>,
    pub entropy_mode: crate::frontend::ast::EntropyMode,
    pub break_requested: bool,
    pub loop_depth: u32,
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
