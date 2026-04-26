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
            causal_history: Vec::new(),
            next_payload_id: 0,
            trace_entropy: false,
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

        let result = self.execute_statement_inner(branch_id, stmt);

        if self.trace_entropy {
            println!("\x1b[1;30m--- Entropy Trace [{}] ---\x1b[0m", branch_id);
            let branch = self.get_branch_mut(branch_id)?;
            let mut keys: Vec<_> = branch.arena.bindings.keys().collect();
            keys.sort();
            for k in keys {
                let state = &branch.arena.bindings[k];
                println!("  \x1b[1;33m{: <10}\x1b[0m: {}", k, state.render_decay(1));
            }
        }

        result
    }

    pub(crate) fn execute_capability(
        &mut self,
        branch_id: &str,
        cap: &Capability,
    ) -> Result<(), TemporalError> {
        // Enforce resource budgets
        let res_name = cap.path.replace(".", "_").to_lowercase();
        {
            let branch = self.get_branch_mut(branch_id)?;
            if let Some(budget) = branch.resource_budgets.get_mut(&res_name) {
                if *budget == 0 {
                    return Err(TemporalError::CapabilityViolation(format!(
                        "Capability budget exhausted: {}",
                        cap.path
                    )));
                }
                *budget -= 1;
            }
        }

        if cap.path == "System.Entropy"
            && cap.parameters.get("mode") == Some(&"chaos".to_string())
        {
            let branch = self.get_branch_mut(branch_id)?;
            branch.entropy_mode = EntropyMode::Chaos;
        }

        if let Some(handler) = self.capability_handlers.get(&cap.path) {
            handler(&cap.parameters).map_err(TemporalError::CapabilityViolation)?;
            Ok(())
        } else if cap.path == "System.Entropy" {
            // System.Entropy is a built-in mode that does not require host handler.
            Ok(())
        } else {
            Err(TemporalError::MissingCapability(cap.path.clone()))
        }
    }

    pub fn split_timeline(
        &mut self,
        parent_id: &str,
        branches: Vec<&str>,
    ) -> Result<(), TemporalError> {
        let (base_arena, cpu_budget_ms, entropy_mode, resource_budgets) = {
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
                parent_timeline.resource_budgets.clone(),
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
                resource_budgets: resource_budgets.clone(),
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
        let mut pending_reversion = None;

        for branch_name in &branches {
            let branch =
                self.active_branches.get(*branch_name).ok_or_else(|| {
                    TemporalError::BranchNotFound(branch_name.to_string())
                })?;
            for (key, state) in &branch.arena.bindings {
                if let Some(existing) = merged_data.get(key) {
                    let strategy = resolution
                        .rules
                        .get(key)
                        .unwrap_or(&ResolutionStrategy::FirstWins);
                    let (resolved, rev) = self.resolve_entropic_conflict(
                        key,
                        existing,
                        state,
                        strategy,
                        branch_name,
                    );
                    merged_data.insert(key.clone(), resolved);
                    if pending_reversion.is_none() {
                        pending_reversion = rev;
                    }
                } else {
                    merged_data.insert(key.clone(), state.clone());
                }
            }
        }

        if let Some(reversion) = pending_reversion {
            // Trigger the reset!
            // We use the same logic as Statement::AcausalReset
            let anchor = {
                let target_branch = self.get_branch_mut(&reversion.branch)?;
                target_branch
                    .anchors
                    .get(&reversion.anchor)
                    .ok_or_else(|| {
                        TemporalError::AnchorNotFound(reversion.anchor.clone())
                    })?
                    .clone()
            };

            let target_branch = self.get_branch_mut(&reversion.branch)?;
            target_branch.arena = anchor.arena_snapshot;
            target_branch.local_clock = anchor.clock_snapshot;
            target_branch.commit_horizon_passed = false;

            // In a more advanced VM we would restart the branch here.
            // For now, we just perform the state reset.
            return Ok(());
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

    pub(crate) fn resolve_entropic_conflict(
        &self,
        _key: &str,
        existing: &EntropicState,
        incoming: &EntropicState,
        strategy: &ResolutionStrategy,
        incoming_branch: &str,
    ) -> (EntropicState, Option<crate::frontend::ast::CausalReversion>) {
        // Causal safety: if either side is already Consumed, it stays Consumed
        if matches!(existing, EntropicState::Consumed)
            || matches!(incoming, EntropicState::Consumed)
        {
            return (EntropicState::Consumed, None);
        }

        match strategy {
            ResolutionStrategy::FirstWins => (existing.clone(), None),
            ResolutionStrategy::Priority(p) => {
                if incoming_branch == p {
                    (incoming.clone(), None)
                } else {
                    (existing.clone(), None)
                }
            }
            ResolutionStrategy::Decay => (EntropicState::Consumed, None),
            ResolutionStrategy::Auto => {
                if existing == incoming {
                    (existing.clone(), None)
                } else {
                    // Entropic collapse on conflict
                    (EntropicState::Consumed, None)
                }
            }
            ResolutionStrategy::TopologyUnion {
                key_rules,
                default,
                on_invalid,
            } => match (existing, incoming) {
                (
                    EntropicState::Valid(Payload::Struct(e_fields))
                    | EntropicState::Valid(Payload::Topology(e_fields))
                    | EntropicState::Decayed(e_fields),
                    EntropicState::Valid(Payload::Struct(i_fields))
                    | EntropicState::Valid(Payload::Topology(i_fields))
                    | EntropicState::Decayed(i_fields),
                ) => {
                    let mut merged_fields = e_fields.clone();
                    let mut triggered_reversion = None;

                    for (k, i_val) in i_fields {
                        if let Some(e_val) = merged_fields.get(k) {
                            let sub_strat = key_rules.get(k).unwrap_or(default);
                            let (resolved, rev) = self.resolve_entropic_conflict(
                                k,
                                e_val,
                                i_val,
                                sub_strat,
                                incoming_branch,
                            );
                            merged_fields.insert(k.clone(), resolved);
                            if triggered_reversion.is_none() {
                                triggered_reversion = rev;
                            }
                        } else {
                            merged_fields.insert(k.clone(), i_val.clone());
                        }
                    }

                    let final_state = match (existing, incoming) {
                        (EntropicState::Decayed(_), _)
                        | (_, EntropicState::Decayed(_)) => {
                            EntropicState::Decayed(merged_fields)
                        }
                        (EntropicState::Valid(Payload::Topology(_)), _)
                        | (_, EntropicState::Valid(Payload::Topology(_))) => {
                            EntropicState::Valid(Payload::Topology(merged_fields))
                        }
                        _ => EntropicState::Valid(Payload::Struct(merged_fields)),
                    };

                    // Check if we collapsed and have an on_invalid rule
                    if matches!(final_state, EntropicState::Consumed)
                        && triggered_reversion.is_none()
                    {
                        triggered_reversion = on_invalid.clone();
                    }

                    (final_state, triggered_reversion)
                }
                _ => (EntropicState::Consumed, on_invalid.clone()),
            },
            ResolutionStrategy::TopologyIntersect {
                key_rules,
                default,
                on_invalid,
            } => match (existing, incoming) {
                (
                    EntropicState::Valid(Payload::Struct(e_fields)),
                    EntropicState::Valid(Payload::Struct(i_fields)),
                )
                | (
                    EntropicState::Valid(Payload::Topology(e_fields)),
                    EntropicState::Valid(Payload::Topology(i_fields)),
                ) => {
                    let mut merged_fields = HashMap::new();
                    let mut triggered_reversion = None;

                    for (k, e_val) in e_fields {
                        if let Some(i_val) = i_fields.get(k) {
                            let sub_strat = key_rules.get(k).unwrap_or(default);
                            let (resolved, rev) = self.resolve_entropic_conflict(
                                k,
                                e_val,
                                i_val,
                                sub_strat,
                                incoming_branch,
                            );
                            merged_fields.insert(k.clone(), resolved);
                            if triggered_reversion.is_none() {
                                triggered_reversion = rev;
                            }
                        }
                    }

                    let final_state = if matches!(
                        existing,
                        EntropicState::Valid(Payload::Topology(_))
                    ) {
                        EntropicState::Valid(Payload::Topology(merged_fields))
                    } else {
                        EntropicState::Valid(Payload::Struct(merged_fields))
                    };

                    (final_state, triggered_reversion)
                }
                _ => (EntropicState::Consumed, on_invalid.clone()),
            },
            ResolutionStrategy::Custom(_) => (existing.clone(), None),
        }
    }

    pub(crate) fn causal_rollback(
        &mut self,
        branch_id: &str,
        start_index: usize,
    ) -> Result<(), TemporalError> {
        for i in (start_index..self.causal_history.len()).rev() {
            let event = self.causal_history[i].clone();
            match event {
                crate::runtime::vm::state::CausalEvent::ChannelSend {
                    branch_id: b_id,
                    channel_id,
                    payload_id,
                } if b_id == branch_id => {
                    let payload_id_val = payload_id;
                    let channel_id_val = channel_id.clone();

                    // Was this specific payload ever received by anyone?
                    let was_received = self.causal_history.iter().skip(i + 1).any(|e| {
                        matches!(e, crate::runtime::vm::state::CausalEvent::ChannelRecv { channel_id: c_id, message, .. }
                            if c_id == &channel_id_val && message.id == payload_id_val)
                    });

                    if was_received {
                        return Err(TemporalError::Paradox);
                    }

                    // Attempt to un-send: remove from channel or pending_channels
                    let mut found = false;
                    if let Some(chan) = self.channels.get_mut(&channel_id_val) {
                        if let Some(pos) =
                            chan.iter().position(|m| m.id == payload_id_val)
                        {
                            chan.remove(pos);
                            found = true;
                        }
                    }
                    if !found {
                        if let Some(pending) =
                            self.pending_channels.get_mut(&channel_id_val)
                        {
                            if let Some(pos) =
                                pending.iter().position(|m| m.id == payload_id_val)
                            {
                                pending.remove(pos);
                                found = true;
                            }
                        }
                    }

                    if !found {
                        return Err(TemporalError::Paradox);
                    }
                }
                crate::runtime::vm::state::CausalEvent::ChannelRecv {
                    branch_id: b_id,
                    channel_id,
                    message,
                } if b_id == branch_id => {
                    // Undo receive by pushing back to front of channel
                    if let Some(chan) = self.channels.get_mut(&channel_id) {
                        chan.push_front(message.clone());
                    } else {
                        return Err(TemporalError::Paradox);
                    }
                }
                crate::runtime::vm::state::CausalEvent::InterBranchMove {
                    source_branch,
                    target_branch,
                    var_name,
                    message: _,
                } if source_branch == branch_id => {
                    // Causal check: if target_branch still has var_name as Valid, we can pull it back
                    let target = self.get_branch_mut(&target_branch)?;
                    match target.arena.bindings.get(&var_name) {
                        Some(EntropicState::Valid(_)) => {
                            target.arena.bindings.remove(&var_name);
                        }
                        _ => {
                            // Paradox: target branch already moved or decayed the variable
                            return Err(TemporalError::Paradox);
                        }
                    }
                }
                _ => {}
            }
        }
        Ok(())
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
            resource_budgets: HashMap::new(),
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
