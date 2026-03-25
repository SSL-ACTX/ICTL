use crate::frontend::ast::{
    Capability, EntropyMode, Expression, MergeResolution, ParamMode,
    ResolutionStrategy, SpeculationCommitMode, Statement, TimeCoordinate,
};
use crate::runtime::gc::GarbageCollector;
use crate::runtime::memory::{Arena, EntropicState, MemoryError, Payload};
use crate::runtime::vm::error::TemporalError;
use crate::runtime::vm::state::{
    AnchorPoint, Routine, SpeculationContext, Timeline, Vm,
};

use std::collections::{HashMap, VecDeque};

impl Vm {
    pub fn new() -> Self {
        Self {
            global_clock: 0,
            root_timeline: Timeline::new("main".to_string(), 1024 * 1024, 0),
            active_branches: HashMap::new(),
            capability_handlers: HashMap::new(),
            channels: HashMap::new(),
            pending_channels: HashMap::new(),
            routines: HashMap::new(),
            speculation_stack: Vec::new(),
            speculative_commit_mode: SpeculationCommitMode::Selective,
            entanglements: Vec::new(),
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

        self.execute_statement_inner(branch_id, stmt)
    }

    pub(crate) fn execute_capability(
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
                slice_ms: None,
                anchors: HashMap::new(),
                commit_horizon_passed: false,
                manifest_stack: Vec::new(),
                entropy_mode,
                break_requested: false,
                loop_depth: 0,
            };
            self.active_branches
                .insert(branch_name.to_string(), new_branch);

            // Propagate entanglement groups to new branch
            let mut new_entries = Vec::new();
            for group in &self.entanglements {
                let mut found_parent = false;
                let mut vars_to_add = Vec::new();
                for (b, v) in group {
                    if b == parent_id {
                        found_parent = true;
                        vars_to_add.push(v.clone());
                    }
                }
                if found_parent {
                    new_entries.push(vars_to_add);
                }
            }

            for vars in new_entries {
                // Find the right group to add to
                for v in vars {
                    for group in &mut self.entanglements {
                        if group.contains(&(parent_id.to_string(), v.clone())) {
                            group.insert((branch_name.to_string(), v.clone()));
                        }
                    }
                }
            }
        }
        Ok(())
    }

    pub fn propagate_entanglement(
        &mut self,
        source_branch: &str,
        var_name: &str,
    ) -> Result<(), TemporalError> {
        let mut groups_to_propagate = Vec::new();
        for (i, group) in self.entanglements.iter().enumerate() {
            if group.contains(&(source_branch.to_string(), var_name.to_string())) {
                groups_to_propagate.push(i);
            }
        }

        for idx in groups_to_propagate {
            let group = self.entanglements[idx].clone();
            for (target_branch, target_var) in group {
                if target_branch == source_branch && target_var == var_name {
                    continue;
                }
                // Mark as consumed in target branch
                if let Ok(branch) = self.get_branch_mut(&target_branch) {
                    branch.arena.set_consumed(&target_var).ok();
                }
            }
        }
        Ok(())
    }

    pub fn propagate_field_decay(
        &mut self,
        source_branch: &str,
        var_name: &str,
        field_name: &str,
    ) -> Result<(), TemporalError> {
        let mut groups_to_propagate = Vec::new();
        for (i, group) in self.entanglements.iter().enumerate() {
            if group.contains(&(source_branch.to_string(), var_name.to_string())) {
                groups_to_propagate.push(i);
            }
        }

        for idx in groups_to_propagate {
            let group = self.entanglements[idx].clone();
            for (target_branch, target_var) in group {
                if target_branch == source_branch && target_var == var_name {
                    continue;
                }
                // Mark field as consumed in target branch
                if let Ok(branch) = self.get_branch_mut(&target_branch) {
                    branch.arena.consume_field(&target_var, field_name).ok();
                }
            }
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
                            ResolutionStrategy::Auto => { /* Auto defaults to first-wins */
                            }
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

    pub fn commit_tick_buffers(&mut self) {
        for (name, pending) in self.pending_channels.iter_mut() {
            if let Some(chan) = self.channels.get_mut(name) {
                chan.append(pending);
            }
        }
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

    pub(crate) fn get_branch_mut(
        &mut self,
        id: &str,
    ) -> Result<&mut Timeline, TemporalError> {
        if id == "main" {
            Ok(&mut self.root_timeline)
        } else {
            self.active_branches
                .get_mut(id)
                .ok_or_else(|| TemporalError::BranchNotFound(id.to_string()))
        }
    }

    pub(crate) fn set_branch_state(&mut self, id: &str, state: Timeline) {
        if id == "main" {
            self.root_timeline = state;
        } else {
            self.active_branches.insert(id.to_string(), state);
        }
    }

    pub(crate) fn simulate_branch(
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
}

impl Timeline {
    pub fn new(id: String, memory_capacity: u64, birth_time: u64) -> Self {
        Self {
            id,
            birth_global_time: birth_time,
            local_clock: 0,
            arena: Arena::new(memory_capacity),
            cpu_budget_ms: u64::MAX,
            slice_ms: None,
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
