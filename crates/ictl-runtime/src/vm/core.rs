use crate::gc::GarbageCollector;
use crate::vm::error::TemporalError;
use crate::vm::state::{AnchorPoint, Routine, SpeculationContext, Timeline, Vm};
use ictl_core::value::{Arena, EntropicState, MemoryError, Payload, ValueMetadata};
use ictl_core::{
    BinaryOperator, Capability, EntropyMode, Expression, MergeResolution, ParamMode,
    ResolutionStrategy, SpeculationCommitMode, Statement, TimeCoordinate,
};

use std::collections::{HashMap, VecDeque};

impl Default for Vm {
    fn default() -> Self {
        Self::new()
    }
}

impl Vm {
    pub fn new() -> Self {
        Self {
            symbols: HashMap::new(),
            global_clock: 0,
            root_timeline: Timeline::new("main".to_string(), 1024 * 1024, 0),
            active_branches: HashMap::new(),
            capability_handlers: HashMap::new(),
            channels: HashMap::new(),
            pending_channels: HashMap::new(),
            routines: HashMap::new(),
            decay_handlers: HashMap::new(),
            type_decay_limits: HashMap::new(),
            speculation_stack: Vec::new(),
            speculative_commit_mode: SpeculationCommitMode::Selective,
            entanglements: Vec::new(),
            causal_history: Vec::new(),
            next_payload_id: 0,
            trace_entropy: false,
            _is_decaying: false,
        }
    }

    pub fn register_capability<F>(&mut self, path: &str, handler: F)
    where
        F: Fn(&HashMap<String, String>) -> Result<(), String> + 'static,
    {
        self.capability_handlers
            .insert(path.to_string(), Box::new(handler));
    }

    pub fn set_speculative_commit_mode(&mut self, mode: SpeculationCommitMode) {
        self.speculative_commit_mode = mode;
    }

    pub fn execute_program(
        &mut self,
        program: &ictl_frontend::ir::IrProgram,
    ) -> Result<(), TemporalError> {
        self.symbols = program.symbols.clone();
        self.type_decay_limits = program.type_decay_limits.clone();
        // Register routines
        for (name, ir_routine) in &program.routines {
            let routine = Routine {
                params: ir_routine.params.clone(),
                return_type: ir_routine.return_type.clone(),
                taking_ms: ir_routine.taking_ms,
                instructions: ir_routine.instructions.clone(),
            };
            self.routines.insert(name.clone(), routine);
        }

        for block in &program.blocks {
            let branch_id = match &block.time {
                TimeCoordinate::Global(_) => "main",
                TimeCoordinate::Relative(_) => "main",
                TimeCoordinate::Branch(name) => name.as_str(),
            };

            {
                let branch = self.get_branch_mut(branch_id)?;
                branch.instructions = block.instructions.clone();
                branch.pc = 0;
            }

            loop {
                let (pc, len) = {
                    let branch = self.get_branch_mut(branch_id)?;
                    (branch.pc, branch.instructions.len())
                };
                if pc >= len {
                    break;
                }

                self.execute_instruction(branch_id)?;

                let b = self.get_branch_mut(branch_id)?;
                if b.break_requested {
                    let target_depth = b.loop_depth;
                    while b.pc < b.instructions.len() {
                        let instr = &b.instructions[b.pc];
                        match instr {
                            ictl_frontend::ir::Instruction::Loop { .. }
                            | ictl_frontend::ir::Instruction::LoopTick => {
                                b.loop_depth += 1;
                            }
                            ictl_frontend::ir::Instruction::EndLoop {
                                max_ms: _,
                            } => {
                                b.loop_depth -= 1;
                                if b.loop_depth < target_depth {
                                    b.loop_stack.pop();
                                    b.pc += 1;
                                    break;
                                }
                            }
                            ictl_frontend::ir::Instruction::EndLoopTick => {
                                b.loop_depth -= 1;
                                if b.loop_depth < target_depth {
                                    b.pc += 1; // skip the EndLoopTick
                                    break;
                                }
                            }
                            _ => {}
                        }
                        b.pc += 1;
                    }
                    b.break_requested = false;
                }
            }
        }
        Ok(())
    }

    pub fn consume_reg(
        &mut self,
        branch_id: &str,
        reg: u32,
    ) -> Result<(), TemporalError> {
        println!("[DEBUG] consume_reg: branch={}, reg={}", branch_id, reg);
        let mut to_consume = Vec::new();
        to_consume.push((branch_id.to_string(), reg));

        // Find all entangled registers
        let mut entangled_found = true;
        while entangled_found {
            entangled_found = false;
            let current_to_consume = to_consume.clone();
            for set in &self.entanglements {
                if current_to_consume.iter().any(|item| set.contains(item)) {
                    for entangled in set {
                        if !to_consume.contains(entangled) {
                            println!("[DEBUG] Found entangled register: branch={}, reg={}", entangled.0, entangled.1);
                            to_consume.push(entangled.clone());
                            entangled_found = true;
                        }
                    }
                }
            }
        }

        for (b_id, r_id) in to_consume {
            println!("[DEBUG] Actually consuming: branch={}, reg={}", b_id, r_id);
            if let Ok(branch) = self.get_branch_mut(&b_id) {
                branch.arena.consume(r_id).ok(); // Ignore if already consumed
            }
        }
        Ok(())
    }

    pub fn consume_field_reg(
        &mut self,
        branch_id: &str,
        reg: u32,
        field: &str,
    ) -> Result<(), TemporalError> {
        let mut to_consume = Vec::new();
        to_consume.push((branch_id.to_string(), reg));

        // Find all entangled registers
        let mut entangled_found = true;
        while entangled_found {
            entangled_found = false;
            let current_to_consume = to_consume.clone();
            for set in &self.entanglements {
                if current_to_consume.iter().any(|item| set.contains(item)) {
                    for entangled in set {
                        if !to_consume.contains(entangled) {
                            to_consume.push(entangled.clone());
                            entangled_found = true;
                        }
                    }
                }
            }
        }

        for (b_id, r_id) in to_consume {
            if let Ok(branch) = self.get_branch_mut(&b_id) {
                branch.arena.consume_field(r_id, field).ok();
            }
        }
        Ok(())
    }

    pub fn execute_instruction(
        &mut self,
        branch_id: &str,
    ) -> Result<(), TemporalError> {
        // Deterministic instruction cost
        {
            let branch = self.get_branch_mut(branch_id)?;
            branch.local_clock += 1;
        }

        let instr = {
            let branch = self.get_branch_mut(branch_id)?;
            if branch.pc >= branch.instructions.len() {
                return Ok(());
            }
            branch.instructions[branch.pc].clone()
        };

        // Advance PC before execution to handle jumps correctly
        {
            let branch = self.get_branch_mut(branch_id)?;
            branch.pc += 1;
        }

        match instr {
            ictl_frontend::ir::Instruction::LoadInt { dest, value } => {
                let branch = self.get_branch_mut(branch_id)?;
                branch
                    .arena
                    .insert(dest.0, EntropicState::Valid(Payload::Integer(value)))?;
            }
            ictl_frontend::ir::Instruction::LoadBool { dest, value } => {
                let branch = self.get_branch_mut(branch_id)?;
                branch
                    .arena
                    .insert(dest.0, EntropicState::Valid(Payload::Bool(value)))?;
            }
            ictl_frontend::ir::Instruction::LoadString { dest, value } => {
                let branch = self.get_branch_mut(branch_id)?;
                branch
                    .arena
                    .insert(dest.0, EntropicState::Valid(Payload::String(value)))?;
            }
            ictl_frontend::ir::Instruction::LoadNull { dest } => {
                let branch = self.get_branch_mut(branch_id)?;
                branch
                    .arena
                    .insert(dest.0, EntropicState::Valid(Payload::Null))?;
            }
            ictl_frontend::ir::Instruction::Move { dest, src } => {
                let state = {
                    let branch = self.get_branch_mut(branch_id)?;
                    branch
                        .arena
                        .registers
                        .get(src.0 as usize)
                        .cloned()
                        .unwrap_or(EntropicState::Consumed)
                };
                if matches!(state, EntropicState::Consumed) {
                    return Err(TemporalError::MemoryFault(
                        MemoryError::AlreadyConsumed,
                    ));
                }
                let branch = self.get_branch_mut(branch_id)?;
                branch.arena.insert(dest.0, state)?;
            }
            ictl_frontend::ir::Instruction::BinaryOp {
                dest,
                op,
                left,
                right,
            } => {
                let l_val = {
                    let branch = self.get_branch_mut(branch_id)?;
                    branch.arena.peek(left.0).ok_or(TemporalError::MemoryFault(
                        MemoryError::AlreadyConsumed,
                    ))?
                };
                let r_val =
                    {
                        let branch = self.get_branch_mut(branch_id)?;
                        branch.arena.peek(right.0).ok_or(
                            TemporalError::MemoryFault(MemoryError::AlreadyConsumed),
                        )?
                    };
                let result = self.evaluate_binary_operation(l_val, r_val, &op)?;
                let branch = self.get_branch_mut(branch_id)?;
                branch.arena.insert(dest.0, EntropicState::Valid(result))?;
            }
            ictl_frontend::ir::Instruction::Jump { target } => {
                let branch = self.get_branch_mut(branch_id)?;
                branch.pc = target;
            }
            ictl_frontend::ir::Instruction::JumpIf { cond, target } => {
                let val = {
                    let branch = self.get_branch_mut(branch_id)?;
                    branch.arena.peek(cond.0).ok_or(TemporalError::MemoryFault(
                        MemoryError::AlreadyConsumed,
                    ))?
                };
                if let Payload::Bool(true) = val {
                    let branch = self.get_branch_mut(branch_id)?;
                    branch.pc = target;
                }
            }
            ictl_frontend::ir::Instruction::JumpIfNot { cond, target } => {
                let val = {
                    let branch = self.get_branch_mut(branch_id)?;
                    branch.arena.peek(cond.0).ok_or(TemporalError::MemoryFault(
                        MemoryError::AlreadyConsumed,
                    ))?
                };
                if let Payload::Bool(false) = val {
                    let branch = self.get_branch_mut(branch_id)?;
                    branch.pc = target;
                }
            }
            ictl_frontend::ir::Instruction::Print { src } => {
                let val = {
                    let branch = self.get_branch_mut(branch_id)?;
                    println!(
                        "[DEBUG] Print reg {}: state={:?}",
                        src.0,
                        branch.arena.registers.get(src.0 as usize)
                    );
                    branch.arena.peek(src.0).ok_or(TemporalError::MemoryFault(
                        MemoryError::AlreadyConsumed,
                    ))?
                };
                let message = val.to_string();
                let cap = Capability {
                    path: "System.Log".to_string(),
                    parameters: [("message".to_string(), message)].into(),
                };
                self._execute_capability(branch_id, &cap)?;
            }
            ictl_frontend::ir::Instruction::Debug { src } => {
                let val = {
                    let branch = self.get_branch_mut(branch_id)?;
                    println!(
                        "[DEBUG] Debug reg {}: state={:?}",
                        src.0,
                        branch.arena.registers.get(src.0 as usize)
                    );
                    branch.arena.peek(src.0).ok_or(TemporalError::MemoryFault(
                        MemoryError::AlreadyConsumed,
                    ))?
                };
                println!("[DEBUG] {}: {:?}", branch_id, val);
                let message = format!("{:?}", val);
                let cap = Capability {
                    path: "System.Log".to_string(),
                    parameters: [("message".to_string(), message)].into(),
                };
                self._execute_capability(branch_id, &cap)?;
            }
            ictl_frontend::ir::Instruction::Merge {
                branches,
                target,
                resolution,
            } => {
                let branch_names: Vec<&str> =
                    branches.iter().map(|s| s.as_str()).collect();
                self.merge_timelines(branch_names, &target, &resolution)?;
            }
            ictl_frontend::ir::Instruction::Entangle { regs } => {
                let mut set = std::collections::HashSet::new();
                for reg in regs {
                    set.insert((branch_id.to_string(), reg.0));
                }
                // Check if any of these registers are already entangled
                let mut existing_set_idx = None;
                for (i, entangled_set) in self.entanglements.iter().enumerate() {
                    if entangled_set
                        .iter()
                        .any(|(b, r)| set.contains(&(b.clone(), *r)))
                    {
                        existing_set_idx = Some(i);
                        break;
                    }
                }

                if let Some(idx) = existing_set_idx {
                    self.entanglements[idx].extend(set);
                } else {
                    self.entanglements.push(set);
                }
            }
            ictl_frontend::ir::Instruction::Anchor { name } => {
                let history_index = self.causal_history.len();
                let branch = self.get_branch_mut(branch_id)?;
                let snapshot = AnchorPoint {
                    name: name.clone(),
                    clock_snapshot: branch.local_clock,
                    arena_snapshot: branch.arena.clone(),
                    cpu_budget_snapshot: branch.cpu_budget_ms,
                    resource_budgets_snapshot: branch.resource_budgets.clone(),
                    history_index,
                    pc_snapshot: branch.pc,
                    instructions_snapshot: branch.instructions.clone(),
                };
                branch.anchors.insert(name, snapshot);
            }
            ictl_frontend::ir::Instruction::Rewind { target, anchor } => {
                let target_id = if target == "self" { branch_id } else { &target };
                let anchor_data = {
                    let t_branch = self.get_branch_mut(target_id)?;
                    t_branch.anchors.get(&anchor).cloned().ok_or_else(|| {
                        TemporalError::AnchorNotFound(anchor.clone())
                    })?
                };

                // Perform causal rollback
                self._causal_rollback(target_id, anchor_data.history_index)?;

                let t_branch = self.get_branch_mut(target_id)?;
                t_branch.arena = anchor_data.arena_snapshot;
                t_branch.local_clock = anchor_data.clock_snapshot;
                t_branch.cpu_budget_ms = anchor_data.cpu_budget_snapshot;
                t_branch.resource_budgets = anchor_data.resource_budgets_snapshot;
                t_branch.pc = anchor_data.pc_snapshot;
                t_branch.instructions = anchor_data.instructions_snapshot;
                t_branch.commit_horizon_passed = false;
            }
            ictl_frontend::ir::Instruction::RelativisticBlock {
                target,
                block_pc,
                block_len,
            } => {
                println!(
                    "[VM] RelativisticBlock: target={}, pc={}, len={}",
                    target, block_pc, block_len
                );
                let (target_id, old_pc, old_instrs) = {
                    let t = self.get_branch_mut(&target)?;
                    let old_pc = t.pc;
                    let old_instrs = t.instructions.clone();
                    (target.clone(), old_pc, old_instrs)
                };

                {
                    let current_instrs =
                        self.get_branch_mut(branch_id)?.instructions.clone();
                    let t = self.get_branch_mut(&target_id)?;
                    t.instructions = current_instrs;
                    t.pc = block_pc;
                }

                for i in 0..block_len {
                    let pc = self.get_branch_mut(&target_id)?.pc;
                    if pc < block_pc || pc >= block_pc + block_len {
                        println!("[VM] RelativisticBlock: PC {} out of bounds [{}, {}), stopping.", pc, block_pc, block_pc + block_len);
                        break;
                    }
                    println!("[VM] RelativisticBlock execution: step {}/{} on {} at PC {}", i+1, block_len, target_id, pc);
                    self.execute_instruction(&target_id)?;
                }

                let t = self.get_branch_mut(&target_id)?;
                t.instructions = old_instrs;
                t.pc = old_pc;
                println!("[VM] RelativisticBlock finished: {}", target_id);
            }
            ictl_frontend::ir::Instruction::Split {
                parent: _,
                branches,
            } => {
                let branch_names: Vec<&str> =
                    branches.iter().map(|s| s.as_str()).collect();
                self.split_timeline(branch_id, branch_names)?;
            }
            ictl_frontend::ir::Instruction::Slice { ms } => {
                let branch = self.get_branch_mut(branch_id)?;
                branch.local_clock += ms;
                branch.slice_ms = Some(ms);
            }
            ictl_frontend::ir::Instruction::Isolate { name: _, manifest } => {
                let (capabilities, cpu_req) = {
                    let branch = self.get_branch_mut(branch_id)?;
                    if let Some(limit_bytes) = manifest.memory_budget_bytes {
                        branch.arena.capacity = limit_bytes;
                    }
                    if let Some(mode) = manifest.mode {
                        branch.entropy_mode = mode;
                    }
                    // Apply resource budgets
                    for (res, amount) in &manifest.resource_budgets {
                        branch.resource_budgets.insert(res.clone(), *amount);
                    }
                    branch.manifest_stack.push(manifest.clone());
                    (manifest.capabilities.clone(), manifest.cpu_budget_ms)
                };

                for cap in &capabilities {
                    self._execute_capability(branch_id, cap)?;
                }

                if let Some(cpu) = cpu_req {
                    let branch = self.get_branch_mut(branch_id)?;
                    if cpu > branch.cpu_budget_ms {
                        return Err(TemporalError::BudgetExhausted);
                    }
                    branch.cpu_budget_ms = cpu;
                    branch.slice_ms = Some(cpu);
                }
            }
            ictl_frontend::ir::Instruction::EndIsolate => {
                let branch = self.get_branch_mut(branch_id)?;
                branch.manifest_stack.pop();
            }
            ictl_frontend::ir::Instruction::Capability { cap } => {
                self._execute_capability(branch_id, &cap)?;
            }
            ictl_frontend::ir::Instruction::For {
                item_name,
                mode,
                source,
                body,
                pacing_ms,
                max_ms,
            } => {
                let source_payload = match mode {
                    ictl_core::ForMode::Consume => {
                        let payload = {
                            let branch = self.get_branch_mut(branch_id)?;
                            branch.arena.peek(source.0).ok_or(
                                TemporalError::MemoryFault(
                                    MemoryError::AlreadyConsumed,
                                ),
                            )?
                        };
                        self.consume_reg(branch_id, source.0)?;
                        payload
                    }
                    ictl_core::ForMode::Clone => {
                        let branch = self.get_branch_mut(branch_id)?;
                        branch.arena.peek(source.0).ok_or(
                            TemporalError::MemoryFault(MemoryError::AlreadyConsumed),
                        )?
                    }
                };

                let elements = match source_payload {
                    Payload::Array(vec) => vec,
                    Payload::Struct(map) | Payload::Topology(map) => {
                        let mut vec = Vec::new();
                        let mut keys: Vec<_> = map.keys().collect();
                        keys.sort();
                        for k in keys {
                            if let Some(EntropicState::Valid(p)) = map.get(k) {
                                let mut fields = HashMap::new();
                                fields.insert(
                                    "key".to_string(),
                                    EntropicState::Valid(Payload::String(k.clone())),
                                );
                                fields.insert(
                                    "value".to_string(),
                                    EntropicState::Valid(p.clone()),
                                );
                                vec.push(Payload::Struct(fields));
                            }
                        }
                        vec
                    }
                    _ => {
                        return Err(TemporalError::EvalError(
                            "for-source must be array or struct".into(),
                        ))
                    }
                };

                let mut elapsed = 0;
                let max_allowed = max_ms.unwrap_or(u64::MAX);

                let item_reg = self.symbols.get(&item_name).unwrap().0;

                for item_value in elements.into_iter() {
                    if elapsed >= max_allowed {
                        break;
                    }

                    {
                        let branch = self.get_branch_mut(branch_id)?;
                        branch
                            .arena
                            .insert(item_reg, EntropicState::Valid(item_value))?;
                    }

                    let iteration_start =
                        self.get_branch_mut(branch_id)?.local_clock;

                    // Execute body
                    let (old_pc, old_instrs) = {
                        let b = self.get_branch_mut(branch_id)?;
                        let pc = b.pc;
                        let instrs = b.instructions.clone();
                        b.instructions = body.clone();
                        b.pc = 0;
                        (pc, instrs)
                    };

                    while {
                        let b = self.get_branch_mut(branch_id)?;
                        b.pc < b.instructions.len()
                    } {
                        self.execute_instruction(branch_id)?;
                    }

                    {
                        let b = self.get_branch_mut(branch_id)?;
                        b.instructions = old_instrs;
                        b.pc = old_pc;
                    }

                    let body_cost = self.get_branch_mut(branch_id)?.local_clock
                        - iteration_start;
                    let paced = pacing_ms.unwrap_or(body_cost);

                    if body_cost > paced {
                        return Err(TemporalError::PacingViolation);
                    }

                    let pad = paced - body_cost;
                    if pad > 0 {
                        let branch = self.get_branch_mut(branch_id)?;
                        branch.local_clock += pad;
                        branch.consume_budget(pad)?;
                    }

                    elapsed += paced;
                }
            }
            ictl_frontend::ir::Instruction::SplitMap {
                item_name,
                mode,
                source,
                body,
                reconcile: _,
            } => {
                let source_payload = match mode {
                    ictl_core::ForMode::Consume => {
                        let payload = {
                            let branch = self.get_branch_mut(branch_id)?;
                            branch.arena.peek(source.0).ok_or(
                                TemporalError::MemoryFault(
                                    MemoryError::AlreadyConsumed,
                                ),
                            )?
                        };
                        self.consume_reg(branch_id, source.0)?;
                        payload
                    }
                    ictl_core::ForMode::Clone => {
                        let branch = self.get_branch_mut(branch_id)?;
                        branch.arena.peek(source.0).ok_or(
                            TemporalError::MemoryFault(MemoryError::AlreadyConsumed),
                        )?
                    }
                };
                let elements = match source_payload {
                    Payload::Array(vec) => vec,
                    Payload::Struct(map) | Payload::Topology(map) => {
                        let mut vec = Vec::new();
                        let mut keys: Vec<_> = map.keys().collect();
                        keys.sort();
                        for k in keys {
                            if let Some(EntropicState::Valid(p)) = map.get(k) {
                                let mut fields = HashMap::new();
                                fields.insert(
                                    "key".to_string(),
                                    EntropicState::Valid(Payload::String(k.clone())),
                                );
                                fields.insert(
                                    "value".to_string(),
                                    EntropicState::Valid(p.clone()),
                                );
                                vec.push(Payload::Struct(fields));
                            }
                        }
                        vec
                    }
                    _ => {
                        return Err(TemporalError::EvalError(
                            "split_map source must be array or struct".into(),
                        ))
                    }
                };

                let mut results: Vec<Payload> = Vec::new();
                let item_reg = self.symbols.get(&item_name).unwrap().0;

                for item_value in elements.into_iter() {
                    let child_name = format!("splitmap_{}", results.len());
                    let child_snapshot = self.get_branch_mut(branch_id)?.clone();

                    self.active_branches
                        .insert(child_name.clone(), child_snapshot);
                    {
                        let child_branch = self.get_branch_mut(&child_name)?;
                        child_branch
                            .arena
                            .insert(item_reg, EntropicState::Valid(item_value))?;
                        child_branch.instructions = body.clone();
                        child_branch.pc = 0;
                    }

                    while {
                        let b = self.get_branch_mut(&child_name)?;
                        b.pc < b.instructions.len()
                    } {
                        self.execute_instruction(&child_name)?;
                    }

                    let child_branch =
                        self.active_branches.remove(&child_name).ok_or_else(
                            || TemporalError::BranchNotFound(child_name.clone()),
                        )?;

                    let yielded = child_branch.arena.peek(0);
                    if let Some(p) = yielded {
                        results.push(p);
                    }
                }

                let out_reg = self
                    .symbols
                    .get("splitmap_results")
                    .expect("splitmap_results not found")
                    .0;
                let branch = self.get_branch_mut(branch_id)?;
                branch.arena.insert(
                    out_reg,
                    EntropicState::Valid(Payload::Array(results)),
                )?;
            }
            ictl_frontend::ir::Instruction::Break => {
                let branch = self.get_branch_mut(branch_id)?;
                branch.break_requested = true;
            }
            ictl_frontend::ir::Instruction::Defer {
                dest,
                cap,
                deadline_ms,
            } => {
                let requested_at = {
                    let branch = self.get_branch_mut(branch_id)?;
                    branch.local_clock
                };
                let latency = cap
                    .parameters
                    .get("latency")
                    .and_then(|l| l.parse::<u64>().ok())
                    .unwrap_or(10);

                let promise = ictl_core::value::PendingPromise {
                    capability: cap.path.clone(),
                    params: cap.parameters.clone(),
                    requested_at,
                    ready_at: requested_at + latency,
                    deadline_at: requested_at + deadline_ms,
                };
                let branch = self.get_branch_mut(branch_id)?;
                branch
                    .arena
                    .insert(dest.0, EntropicState::Pending(promise))?;
            }
            ictl_frontend::ir::Instruction::Await { target } => {
                let promise = {
                    let branch = self.get_branch_mut(branch_id)?;
                    println!(
                        "[DEBUG] Await reg {}: state={:?}",
                        target.0,
                        branch.arena.registers.get(target.0 as usize)
                    );
                    match branch.arena.registers.get(target.0 as usize) {
                        Some(EntropicState::Pending(p)) => p.clone(),
                        _ => {
                            return Err(TemporalError::EvalError(
                                "await target must be a pending promise".into(),
                            ))
                        }
                    }
                };

                {
                    let branch = self.get_branch_mut(branch_id)?;
                    if branch.local_clock < promise.ready_at {
                        let wait = promise.ready_at - branch.local_clock;
                        branch.local_clock = promise.ready_at;
                        branch.consume_budget(wait)?;
                    }

                    if branch.local_clock > promise.deadline_at {
                        // Timeout!
                        branch.arena.insert(target.0, EntropicState::Consumed)?;
                        return Ok(());
                    }
                }

                // Execute the capability now that it's ready
                let cap = Capability {
                    path: promise.capability,
                    parameters: promise.params,
                };
                self._execute_capability(branch_id, &cap)?;

                // For now, let's say it returns Null
                let branch = self.get_branch_mut(branch_id)?;
                branch
                    .arena
                    .insert(target.0, EntropicState::Valid(Payload::Null))?;
            }
            ictl_frontend::ir::Instruction::Consume { src } => {
                self.consume_reg(branch_id, src.0)?;
            }
            ictl_frontend::ir::Instruction::ConsumeField { src, field } => {
                self.consume_field_reg(branch_id, src.0, &field)?;
            }
            ictl_frontend::ir::Instruction::Clone { dest, src } => {
                let val = {
                    let branch = self.get_branch_mut(branch_id)?;
                    let payload = branch.arena.peek(src.0).ok_or(
                        TemporalError::MemoryFault(MemoryError::AlreadyConsumed),
                    )?;
                    let cost = branch.arena.calculate_clone_cost(&payload, 1);
                    branch.consume_budget(cost)?;
                    payload
                };
                let branch = self.get_branch_mut(branch_id)?;
                branch.arena.insert(dest.0, EntropicState::Valid(val))?;
            }
            ictl_frontend::ir::Instruction::ConsumeFieldDynamic {
                target,
                index,
            } => {
                let idx_val =
                    {
                        let branch = self.get_branch_mut(branch_id)?;
                        branch.arena.peek(index.0).ok_or(
                            TemporalError::MemoryFault(MemoryError::AlreadyConsumed),
                        )?
                    };
                let idx_str = match idx_val {
                    Payload::String(s) => s,
                    Payload::Integer(i) => i.to_string(),
                    _ => {
                        return Err(TemporalError::EvalError(
                            "Index must be string or integer".into(),
                        ))
                    }
                };
                self.consume_field_reg(branch_id, target.0, &idx_str)?;
            }
            ictl_frontend::ir::Instruction::StructLit {
                dest,
                fields,
                type_name,
            } => {
                let mut evaluated_fields = HashMap::new();
                for (name, reg) in fields {
                    let val = {
                        let branch = self.get_branch_mut(branch_id)?;
                        branch.arena.peek(reg.0).ok_or(
                            TemporalError::MemoryFault(MemoryError::AlreadyConsumed),
                        )?
                    };
                    evaluated_fields.insert(name.clone(), EntropicState::Valid(val));
                }

                let decay_after_ms = type_name
                    .as_ref()
                    .and_then(|name| self.type_decay_limits.get(name))
                    .cloned();
                let global_time = self.global_clock;

                let branch = self.get_branch_mut(branch_id)?;
                let meta = ValueMetadata {
                    instantiated_at: global_time + branch.local_clock,
                    type_name: type_name.clone(),
                    decay_after_ms,
                };

                branch.arena.insert_with_metadata(
                    dest.0,
                    EntropicState::Valid(Payload::Struct(evaluated_fields)),
                    meta,
                )?;
            }
            ictl_frontend::ir::Instruction::TopologyLit { dest, fields } => {
                let mut evaluated_fields = HashMap::new();
                for (name, reg) in fields {
                    let val = {
                        let branch = self.get_branch_mut(branch_id)?;
                        branch.arena.peek(reg.0).ok_or(
                            TemporalError::MemoryFault(MemoryError::AlreadyConsumed),
                        )?
                    };
                    evaluated_fields.insert(name.clone(), EntropicState::Valid(val));
                }
                let branch = self.get_branch_mut(branch_id)?;
                branch.arena.insert(
                    dest.0,
                    EntropicState::Valid(Payload::Topology(evaluated_fields)),
                )?;
            }
            ictl_frontend::ir::Instruction::ArrayLit { dest, elements } => {
                let mut values = Vec::new();
                for reg in elements {
                    let val = {
                        let branch = self.get_branch_mut(branch_id)?;
                        branch.arena.peek(reg.0).ok_or(
                            TemporalError::MemoryFault(MemoryError::AlreadyConsumed),
                        )?
                    };
                    values.push(val);
                }
                let branch = self.get_branch_mut(branch_id)?;
                branch
                    .arena
                    .insert(dest.0, EntropicState::Valid(Payload::Array(values)))?;
            }
            ictl_frontend::ir::Instruction::FieldAccess {
                dest,
                target,
                field,
            } => {
                let field_state = {
                    let branch = self.get_branch_mut(branch_id)?;
                    branch
                        .arena
                        .consume_field_entropic(target.0, &field)
                        .map_err(TemporalError::MemoryFault)?
                };
                let branch = self.get_branch_mut(branch_id)?;
                branch.arena.insert(dest.0, field_state)?;
                self.propagate_field_decay(branch_id, target.0, &field)?;
            }
            ictl_frontend::ir::Instruction::FieldUpdate { target, field, src } => {
                let val = {
                    let branch = self.get_branch_mut(branch_id)?;
                    branch.arena.peek(src.0).ok_or(TemporalError::MemoryFault(
                        MemoryError::AlreadyConsumed,
                    ))?
                };
                let branch = self.get_branch_mut(branch_id)?;
                branch.arena.update_field(target.0, &field, val)?;
            }
            ictl_frontend::ir::Instruction::IndexAccess {
                dest,
                target,
                index,
            } => {
                let target_val =
                    {
                        let branch = self.get_branch_mut(branch_id)?;
                        branch.arena.peek(target.0).ok_or(
                            TemporalError::MemoryFault(MemoryError::AlreadyConsumed),
                        )?
                    };
                let idx_val =
                    {
                        let branch = self.get_branch_mut(branch_id)?;
                        branch.arena.peek(index.0).ok_or(
                            TemporalError::MemoryFault(MemoryError::AlreadyConsumed),
                        )?
                    };
                let idx_str = match idx_val {
                    Payload::String(s) => s,
                    Payload::Integer(i) => i.to_string(),
                    _ => {
                        return Err(TemporalError::EvalError(
                            "Index must be string or integer".into(),
                        ))
                    }
                };
                let state = match target_val {
                    Payload::Struct(fields) | Payload::Topology(fields) => fields
                        .get(&idx_str)
                        .cloned()
                        .unwrap_or(EntropicState::Consumed),
                    _ => EntropicState::Consumed,
                };
                let branch = self.get_branch_mut(branch_id)?;
                branch.arena.insert(dest.0, state)?;
            }
            ictl_frontend::ir::Instruction::IndexFieldUpdate {
                target,
                index,
                field,
                src,
            } => {
                let val = {
                    let branch = self.get_branch_mut(branch_id)?;
                    branch.arena.peek(src.0).ok_or(TemporalError::MemoryFault(
                        MemoryError::AlreadyConsumed,
                    ))?
                };
                let idx_val =
                    {
                        let branch = self.get_branch_mut(branch_id)?;
                        branch.arena.peek(index.0).ok_or(
                            TemporalError::MemoryFault(MemoryError::AlreadyConsumed),
                        )?
                    };
                let idx_str = match idx_val {
                    Payload::String(s) => s,
                    Payload::Integer(i) => i.to_string(),
                    _ => {
                        return Err(TemporalError::EvalError(
                            "Index must be string or integer".into(),
                        ))
                    }
                };

                let branch = self.get_branch_mut(branch_id)?;
                branch
                    .arena
                    .update_index_field(target.0, &idx_str, &field, val)?;
            }
            ictl_frontend::ir::Instruction::OpenChan { name, capacity } => {
                self.channels
                    .insert(name.clone(), VecDeque::with_capacity(capacity));
                self.pending_channels
                    .insert(name, VecDeque::with_capacity(capacity));
            }
            ictl_frontend::ir::Instruction::ChanSend { chan_id, src } => {
                let val = {
                    let branch = self.get_branch_mut(branch_id)?;
                    branch.arena.peek(src.0).ok_or(TemporalError::MemoryFault(
                        MemoryError::AlreadyConsumed,
                    ))?
                };
                let message = crate::vm::state::Message {
                    id: self.next_payload_id,
                    sender: branch_id.to_string(),
                    payload: val,
                };
                println!(
                    "[VM] {} sending message {} on chan {}",
                    branch_id, message.id, chan_id
                );
                self.next_payload_id += 1;

                let is_isochronous =
                    self.get_branch_mut(branch_id)?.slice_ms.is_some();

                if is_isochronous {
                    if let Some(pending) = self.pending_channels.get_mut(&chan_id) {
                        pending.push_back(message.clone());
                    } else {
                        return Err(TemporalError::ChannelFault(format!(
                            "Channel not found: {}",
                            chan_id
                        )));
                    }
                } else if let Some(chan) = self.channels.get_mut(&chan_id) {
                    chan.push_back(message.clone());
                } else {
                    return Err(TemporalError::ChannelFault(format!(
                        "Channel not found: {}",
                        chan_id
                    )));
                }

                self.causal_history.push(
                    crate::vm::state::CausalEvent::ChannelSend {
                        branch_id: branch_id.to_string(),
                        channel_id: chan_id,
                        payload_id: message.id,
                    },
                );
            }
            ictl_frontend::ir::Instruction::ChanRecv { dest, chan_id } => {
                let message = {
                    let chan = self.channels.get_mut(&chan_id).ok_or_else(|| {
                        TemporalError::ChannelFault(format!(
                            "Channel not found: {}",
                            chan_id
                        ))
                    })?;
                    chan.pop_front().ok_or_else(|| {
                        TemporalError::ChannelFault(format!(
                            "Channel empty: {}",
                            chan_id
                        ))
                    })?
                };
                println!(
                    "[VM] {} receiving message {} on chan {}",
                    branch_id, message.id, chan_id
                );

                self.causal_history.push(
                    crate::vm::state::CausalEvent::ChannelRecv {
                        branch_id: branch_id.to_string(),
                        channel_id: chan_id,
                        message: message.clone(),
                    },
                );

                let branch = self.get_branch_mut(branch_id)?;
                branch
                    .arena
                    .insert(dest.0, EntropicState::Valid(message.payload))?;
            }
            ictl_frontend::ir::Instruction::Call {
                routine,
                args,
                dest,
            } => {
                let routine_def = self
                    .routines
                    .get(&routine)
                    .ok_or_else(|| {
                        TemporalError::EvalError(format!(
                            "unknown routine {}",
                            routine
                        ))
                    })?
                    .clone();
                let params = routine_def.params.clone();

                if args.len() != params.len() {
                    return Err(TemporalError::EvalError(format!(
                        "routine call expects {} args, got {}",
                        params.len(),
                        args.len()
                    )));
                }

                let mut arg_values = Vec::new();
                for reg in &args {
                    let val = {
                        let branch = self.get_branch_mut(branch_id)?;
                        branch.arena.peek(reg.0).ok_or(
                            TemporalError::MemoryFault(MemoryError::AlreadyConsumed),
                        )?
                    };
                    arg_values.push(val);
                }

                // Consume arguments if needed
                for (i, reg) in args.iter().enumerate() {
                    let (mode, _, _) = &params[i];
                    if let ParamMode::Consume = mode {
                        self.consume_reg(branch_id, reg.0)?;
                    }
                }

                let child_id =
                    format!("__routine_{}_{}", routine, self.global_clock);
                let mut child =
                    Timeline::new(child_id.clone(), 1024 * 1024, self.global_clock);
                child.instructions = routine_def.instructions.clone();

                for (i, (mode, _name, _)) in params.iter().enumerate() {
                    let val = arg_values[i].clone();
                    match mode {
                        ParamMode::Consume | ParamMode::Clone | ParamMode::Peek => {
                            child
                                .arena
                                .insert(i as u32, EntropicState::Valid(val))?;
                        }
                        ParamMode::Decay => {
                            child
                                .arena
                                .insert(i as u32, EntropicState::Valid(val))?;
                        }
                    }
                }

                self.active_branches.insert(child_id.clone(), child);

                while {
                    let b = self.get_branch_mut(&child_id)?;
                    b.pc < b.instructions.len()
                } {
                    self.execute_instruction(&child_id)?;
                }

                let child_branch =
                    self.active_branches.remove(&child_id).ok_or_else(|| {
                        TemporalError::BranchNotFound(child_id.clone())
                    })?;

                let result = child_branch
                    .arena
                    .peek(0)
                    .unwrap_or(Payload::String("void".to_string()));

                let branch = self.get_branch_mut(branch_id)?;
                branch.arena.insert(dest.0, EntropicState::Valid(result))?;
            }
            ictl_frontend::ir::Instruction::Return { src } => {
                let val = if let Some(reg) = src {
                    let branch = self.get_branch_mut(branch_id)?;
                    branch.arena.peek(reg.0).unwrap_or(Payload::Null)
                } else {
                    Payload::Null
                };
                let branch = self.get_branch_mut(branch_id)?;
                branch.arena.insert(0, EntropicState::Valid(val))?;
                branch.pc = branch.instructions.len();
            }
            ictl_frontend::ir::Instruction::LoopTick => {
                self.commit_tick_buffers();
                let branch = self.get_branch_mut(branch_id)?;
                branch.loop_depth += 1;
            }
            ictl_frontend::ir::Instruction::EndLoopTick => {
                let branch = self.get_branch_mut(branch_id)?;
                if branch.loop_depth > 0 {
                    branch.loop_depth -= 1;
                }
            }
            ictl_frontend::ir::Instruction::Loop { max_ms } => {
                let branch = self.get_branch_mut(branch_id)?;
                branch.loop_stack.push((branch.local_clock, max_ms));
                branch.loop_depth += 1;
            }
            ictl_frontend::ir::Instruction::EndLoop { max_ms: _ } => {
                let branch = self.get_branch_mut(branch_id)?;
                branch.loop_stack.pop();
                if branch.loop_depth > 0 {
                    branch.loop_depth -= 1;
                }
            }
            ictl_frontend::ir::Instruction::AssertTime { op, limit_ms } => {
                let branch = self.get_branch_mut(branch_id)?;
                let elapsed = branch.local_clock;
                let condition = match op {
                    BinaryOperator::Lt => elapsed < limit_ms,
                    BinaryOperator::Gt => elapsed > limit_ms,
                    BinaryOperator::Le => elapsed <= limit_ms,
                    BinaryOperator::Ge => elapsed >= limit_ms,
                    BinaryOperator::Eq => elapsed == limit_ms,
                    BinaryOperator::Neq => elapsed != limit_ms,
                    _ => false,
                };
                if !condition {
                    return Err(TemporalError::AssertionFailed(format!(
                        "Temporal assertion failed: elapsed {}ms {:?} {}ms",
                        elapsed, op, limit_ms
                    )));
                }
            }
            ictl_frontend::ir::Instruction::Speculate {
                max_ms: _,
                fallback_target,
            } => {
                let current_timeline = self.get_branch_mut(branch_id)?.clone();
                let history_index = self.causal_history.len();
                self.speculation_stack.push(SpeculationContext {
                    speculation_start_state: current_timeline,
                    history_start_index: history_index,
                    fallback_target,
                    commit_vars: std::collections::HashSet::new(),
                    in_commit_block: false,
                    commit_executed: false,
                    collapse_happened: false,
                });
            }
            ictl_frontend::ir::Instruction::EndSpeculate {
                max_ms: _,
                fallback_target: _,
            } => {
                let context =
                    self.speculation_stack
                        .pop()
                        .ok_or(TemporalError::EvalError(
                            "EndSpeculate without Speculate".into(),
                        ))?;
                if !context.commit_executed
                    && self.speculative_commit_mode
                        == SpeculationCommitMode::Selective
                {
                    // Rollback if selective mode and no commit
                    let branch = self.get_branch_mut(branch_id)?;
                    let current_pc = branch.pc;
                    let current_instrs = branch.instructions.clone();
                    let current_loop_depth = branch.loop_depth;
                    let current_break = branch.break_requested;

                    *branch = context.speculation_start_state;

                    branch.pc = current_pc;
                    branch.instructions = current_instrs;
                    branch.loop_depth = current_loop_depth;
                    branch.break_requested = current_break;
                }
            }
            ictl_frontend::ir::Instruction::Select {
                max_ms: _,
                cases,
                timeout_target,
            } => {
                let mut found_case = None;
                for case in &cases {
                    let message = {
                        if let Some(chan) = self.channels.get_mut(&case.chan_id) {
                            chan.pop_front()
                        } else {
                            None
                        }
                    };

                    if let Some(msg) = message {
                        self.causal_history.push(
                            crate::vm::state::CausalEvent::ChannelRecv {
                                branch_id: branch_id.to_string(),
                                channel_id: case.chan_id.clone(),
                                message: msg.clone(),
                            },
                        );

                        let branch = self.get_branch_mut(branch_id)?;
                        branch.arena.insert(
                            case.dest.0,
                            EntropicState::Valid(msg.payload),
                        )?;
                        found_case = Some(case.target);
                        break;
                    }
                }

                if let Some(target) = found_case {
                    let branch = self.get_branch_mut(branch_id)?;
                    branch.pc = target;
                } else if let Some(target) = timeout_target {
                    // For now, if no message is ready, we just jump to timeout immediately
                    // A real implementation might wait or retry if local_clock < birth + max_ms
                    let branch = self.get_branch_mut(branch_id)?;
                    branch.pc = target;
                }
            }
            ictl_frontend::ir::Instruction::MatchEntropy {
                target,
                valid_target,
                decayed_target,
                pending_target,
                consumed_target,
            } => {
                let state = {
                    let global_time = self.global_clock;
                    let branch = self.get_branch_mut(branch_id)?;
                    let current_time = global_time + branch.local_clock;

                    // Check for temporal decay
                    if let Some(Some(meta)) =
                        branch.arena.metadata.get(target.0 as usize)
                    {
                        if let Some(limit) = meta.decay_after_ms {
                            if current_time >= meta.instantiated_at + limit {
                                println!("[DEBUG] Temporal decay triggered for reg {} at time {}", target.0, current_time);
                                branch.arena.decay(target.0).ok();
                            }
                        }
                    }

                    let s = branch
                        .arena
                        .registers
                        .get(target.0 as usize)
                        .cloned()
                        .unwrap_or(EntropicState::Consumed);
                    println!(
                        "[DEBUG] MatchEntropy branch={}, reg={}, state={:?}",
                        branch_id, target.0, s
                    );
                    s
                };

                let maybe_jump = match state {
                    EntropicState::Valid(_) => valid_target,
                    EntropicState::Decayed(_) => decayed_target,
                    EntropicState::Pending(_) => pending_target,
                    EntropicState::Consumed => consumed_target,
                };

                if let Some(target_pc) = maybe_jump {
                    println!("[DEBUG] MatchEntropy jumping to PC {}", target_pc);
                    let branch = self.get_branch_mut(branch_id)?;
                    branch.pc = target_pc;
                } else {
                    println!("[DEBUG] MatchEntropy NO JUMP for state {:?}", state);
                }
            }
            ictl_frontend::ir::Instruction::Collapse => {
                let context =
                    self.speculation_stack
                        .pop()
                        .ok_or(TemporalError::EvalError(
                            "Collapse outside speculation".into(),
                        ))?;
                let fallback_target = context.fallback_target;
                let start_state = context.speculation_start_state;

                let branch = self.get_branch_mut(branch_id)?;
                let saved_instructions = branch.instructions.clone();
                *branch = start_state;
                branch.instructions = saved_instructions;
                branch.pc = fallback_target;
            }
            ictl_frontend::ir::Instruction::Commit { vars: _ } => {
                if let Some(context) = self.speculation_stack.last_mut() {
                    context.commit_executed = true;
                }
            }
            ictl_frontend::ir::Instruction::SpeculationMode { mode } => {
                self.speculative_commit_mode = mode;
            }
            ictl_frontend::ir::Instruction::Watchdog {
                target,
                timeout_ms,
                recovery_jump,
            } => {
                let (target_clock, is_active) = {
                    if let Ok(t) = self.get_branch_mut(&target) {
                        (t.local_clock, true)
                    } else {
                        (0, false)
                    }
                };

                if is_active && target_clock > timeout_ms {
                    if let Some(jump) = recovery_jump {
                        let branch = self.get_branch_mut(branch_id)?;
                        branch.pc = jump;
                    }
                }
            }
            ictl_frontend::ir::Instruction::NetworkRequest { domain: _ } => {
                if !self.capability_handlers.contains_key("System.NetworkFetch") {
                    return Err(TemporalError::MissingCapability(
                        "System.NetworkFetch".to_string(),
                    ));
                }
                let branch = self.get_branch_mut(branch_id)?;
                branch.local_clock += 5;
                branch.consume_budget(5)?;
            }
        }

        if self.trace_entropy {
            println!("\x1b[1;30m--- Entropy Trace [{}] ---\x1b[0m", branch_id);
            let branch = self.get_branch_mut(branch_id)?;
            for (i, state) in branch.arena.registers.iter().enumerate() {
                if !matches!(state, EntropicState::Consumed) {
                    println!(
                        "  \x1b[1;33mR{: <10}\x1b[0m: {}",
                        i,
                        state.render_decay(1)
                    );
                }
            }
        }

        Ok(())
    }

    pub(crate) fn evaluate_binary_operation(
        &self,
        left_value: Payload,
        right_value: Payload,
        op: &BinaryOperator,
    ) -> Result<Payload, TemporalError> {
        let result = match (left_value, right_value) {
            (Payload::Integer(l), Payload::Integer(r)) => match op {
                BinaryOperator::Add => Payload::Integer(l + r),
                BinaryOperator::Sub => Payload::Integer(l - r),
                BinaryOperator::Mul => Payload::Integer(l * r),
                BinaryOperator::Div => {
                    if r == 0 {
                        return Err(TemporalError::EvalError(
                            "Division by zero".into(),
                        ));
                    }
                    Payload::Integer(l / r)
                }
                BinaryOperator::Eq => Payload::Bool(l == r),
                BinaryOperator::Neq => Payload::Bool(l != r),
                BinaryOperator::Lt => Payload::Bool(l < r),
                BinaryOperator::Gt => Payload::Bool(l > r),
                BinaryOperator::Le => Payload::Bool(l <= r),
                BinaryOperator::Ge => Payload::Bool(l >= r),
            },
            (Payload::Bool(l), Payload::Bool(r)) => match op {
                BinaryOperator::Eq => Payload::Bool(l == r),
                BinaryOperator::Neq => Payload::Bool(l != r),
                _ => {
                    return Err(TemporalError::EvalError(
                        "Invalid boolean operator".into(),
                    ))
                }
            },
            (Payload::String(l), Payload::String(r)) => match op {
                BinaryOperator::Eq => Payload::Bool(l == r),
                BinaryOperator::Neq => Payload::Bool(l != r),
                _ => {
                    return Err(TemporalError::EvalError(
                        "String operator unsupported".into(),
                    ))
                }
            },
            (l, r) => {
                return Err(TemporalError::TypeMismatch(format!(
                    "Type mismatch in binary op: {:?} {:?} {:?}",
                    l, op, r
                )));
            }
        };

        Ok(result)
    }

    pub(crate) fn _execute_capability(
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
                loop_stack: Vec::new(),
                pc: 0,
                instructions: Vec::new(),
            };
            self.active_branches
                .insert(branch_name.to_string(), new_branch);

            // Propagate entanglement groups to new branch
            let mut new_entries = Vec::new();
            for group in &self.entanglements {
                let mut found_parent = false;
                let mut regs_to_add = Vec::new();
                for (b, r) in group {
                    if b == parent_id {
                        found_parent = true;
                        regs_to_add.push(*r);
                    }
                }
                if found_parent {
                    new_entries.push(regs_to_add);
                }
            }

            for regs in new_entries {
                for r in regs {
                    for group in &mut self.entanglements {
                        if group.contains(&(parent_id.to_string(), r)) {
                            group.insert((branch_name.to_string(), r));
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
        reg: u32,
    ) -> Result<(), TemporalError> {
        let mut groups_to_propagate = Vec::new();
        for (i, group) in self.entanglements.iter().enumerate() {
            if group.contains(&(source_branch.to_string(), reg)) {
                groups_to_propagate.push(i);
            }
        }

        for idx in groups_to_propagate {
            let group = self.entanglements[idx].clone();
            for (target_branch, target_reg) in group {
                if target_branch == source_branch && target_reg == reg {
                    continue;
                }
                // Mark as consumed in target branch
                if let Ok(branch) = self.get_branch_mut(&target_branch) {
                    branch.arena.set_consumed(target_reg).ok();
                }
            }
        }
        Ok(())
    }

    pub fn propagate_field_decay(
        &mut self,
        source_branch: &str,
        reg: u32,
        field_name: &str,
    ) -> Result<(), TemporalError> {
        let mut groups_to_propagate = Vec::new();
        for (i, group) in self.entanglements.iter().enumerate() {
            if group.contains(&(source_branch.to_string(), reg)) {
                groups_to_propagate.push(i);
            }
        }

        for idx in groups_to_propagate {
            let group = self.entanglements[idx].clone();
            for (target_branch, target_reg) in group {
                if target_branch == source_branch && target_reg == reg {
                    continue;
                }
                // Mark field as consumed in target branch
                if let Ok(branch) = self.get_branch_mut(&target_branch) {
                    branch.arena.consume_field(target_reg, field_name).ok();
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
        println!(
            "[DEBUG] merge_timelines: branches={:?}, target={}",
            branches, target
        );
        let mut merged_registers: Vec<Option<EntropicState>> = Vec::new();
        let mut pending_reversion = None;

        // Build a mapping from register ID to resolution strategy
        let mut reg_resolutions: HashMap<u32, ResolutionStrategy> = HashMap::new();
        for (name, strategy) in &resolution.rules {
            if let Some(reg) = self.symbols.get(name) {
                println!(
                    "[DEBUG] Rule for variable {}: strategy at reg {}",
                    name, reg.0
                );
                reg_resolutions.insert(reg.0, strategy.clone());
            } else {
                println!(
                    "[DEBUG] Rule for variable {} BUT NOT FOUND IN SYMBOLS",
                    name
                );
            }
        }

        for branch_name in &branches {
            println!("[DEBUG] Merging branch {}", branch_name);
            let branch =
                self.active_branches.get(*branch_name).ok_or_else(|| {
                    TemporalError::BranchNotFound(branch_name.to_string())
                })?;

            if merged_registers.len() < branch.arena.registers.len() {
                merged_registers.resize(branch.arena.registers.len(), None);
            }

            for (idx, state) in branch.arena.registers.iter().enumerate() {
                if let Some(existing) = &merged_registers[idx] {
                    let strategy = reg_resolutions
                        .get(&(idx as u32))
                        .unwrap_or(&ResolutionStrategy::Auto);
                    println!("[DEBUG] Conflict at reg {}: existing={:?}, incoming={:?}, strategy={:?}", idx, existing, state, strategy);
                    let (resolved, rev) = self.resolve_entropic_conflict(
                        &idx.to_string(),
                        existing,
                        state,
                        strategy,
                        branch_name,
                    );
                    println!("[DEBUG] Resolved reg {} to {:?}", idx, resolved);
                    merged_registers[idx] = Some(resolved);
                    if pending_reversion.is_none() {
                        pending_reversion = rev;
                    }
                } else {
                    println!(
                        "[DEBUG] Copying reg {} from {}: {:?}",
                        idx, branch_name, state
                    );
                    merged_registers[idx] = Some(state.clone());
                }
            }
        }

        if let Some(reversion) = pending_reversion {
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
            target_branch.cpu_budget_ms = anchor.cpu_budget_snapshot;
            target_branch.resource_budgets = anchor.resource_budgets_snapshot;
            target_branch.commit_horizon_passed = false;
            target_branch.pc = anchor.pc_snapshot;

            return Ok(());
        }

        let target_branch = self.get_branch_mut(target)?;
        for (idx, v) in merged_registers.into_iter().enumerate() {
            if let Some(state) = v {
                target_branch.arena.insert(idx as u32, state)?;
            }
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
    ) -> (EntropicState, Option<ictl_core::CausalReversion>) {
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
            ResolutionStrategy::TopologyUnion {
                key_rules,
                default,
                on_invalid,
            } => {
                match (existing, incoming) {
                    (
                        EntropicState::Valid(Payload::Topology(f1)),
                        EntropicState::Valid(Payload::Topology(f2)),
                    ) => {
                        let mut merged_fields = f1.clone();
                        let mut final_reversion = None;

                        for (field_name, incoming_f_state) in f2 {
                            if let Some(existing_f_state) =
                                merged_fields.get(field_name)
                            {
                                let field_strategy =
                                    key_rules.get(field_name).unwrap_or(default);
                                let (resolved_f, rev) = self
                                    .resolve_entropic_conflict(
                                        field_name,
                                        existing_f_state,
                                        incoming_f_state,
                                        field_strategy,
                                        incoming_branch,
                                    );
                                merged_fields.insert(field_name.clone(), resolved_f);
                                if final_reversion.is_none() {
                                    final_reversion = rev;
                                }
                            } else {
                                merged_fields.insert(
                                    field_name.clone(),
                                    incoming_f_state.clone(),
                                );
                            }
                        }

                        // Check if any merged fields became Consumed and if we should revert
                        if merged_fields
                            .values()
                            .any(|s| matches!(s, EntropicState::Consumed))
                        {
                            if let Some(rev) = on_invalid {
                                return (EntropicState::Consumed, Some(rev.clone()));
                            }
                        }

                        (
                            EntropicState::Valid(Payload::Topology(merged_fields)),
                            final_reversion,
                        )
                    }
                    _ => (EntropicState::Consumed, on_invalid.clone()),
                }
            }
            ResolutionStrategy::Auto => {
                if existing == incoming {
                    (existing.clone(), None)
                } else {
                    (EntropicState::Consumed, None)
                }
            }
            _ => (existing.clone(), None),
        }
    }

    pub(crate) fn _causal_rollback(
        &mut self,
        branch_id: &str,
        start_index: usize,
    ) -> Result<(), TemporalError> {
        for i in (start_index..self.causal_history.len()).rev() {
            let event = self.causal_history[i].clone();
            match event {
                crate::vm::state::CausalEvent::ChannelSend {
                    branch_id: b_id,
                    channel_id,
                    payload_id,
                } if b_id == branch_id => {
                    let payload_id_val = payload_id;
                    let channel_id_val = channel_id.clone();

                    let was_received = self.causal_history.iter().skip(i + 1).any(|e| {
                        match e {
                            crate::vm::state::CausalEvent::ChannelRecv { channel_id: c_id, message, .. } => {
                                let match_found = c_id == &channel_id_val && message.id == payload_id_val;
                                if match_found {
                                    println!("[VM] Paradox detected: message {} on chan {} was received by {} after send was rolled back", payload_id_val, channel_id_val, b_id);
                                }
                                match_found
                            }
                            _ => false,
                        }
                    });

                    if was_received {
                        return Err(TemporalError::Paradox);
                    }

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
                crate::vm::state::CausalEvent::ChannelRecv {
                    branch_id: b_id,
                    channel_id,
                    message,
                } if b_id == branch_id => {
                    if let Some(chan) = self.channels.get_mut(&channel_id) {
                        chan.push_front(message.clone());
                    } else {
                        return Err(TemporalError::Paradox);
                    }
                }
                crate::vm::state::CausalEvent::InterBranchMove {
                    source_branch,
                    target_branch,
                    reg,
                    message: _,
                } if source_branch == branch_id => {
                    let target = self.get_branch_mut(&target_branch)?;
                    match target.arena.registers.get(reg as usize) {
                        Some(EntropicState::Valid(_)) => {
                            target.arena.registers[reg as usize] =
                                EntropicState::Consumed;
                        }
                        _ => {
                            return Err(TemporalError::Paradox);
                        }
                    }
                }
                _ => {}
            }
        }
        Ok(())
    }

    pub(crate) fn _set_branch_state(&mut self, id: &str, state: Timeline) {
        if id == "main" {
            self.root_timeline = state;
        } else {
            self.active_branches.insert(id.to_string(), state);
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
            slice_ms: None,
            anchors: HashMap::new(),
            commit_horizon_passed: false,
            manifest_stack: Vec::new(),
            resource_budgets: HashMap::new(),
            entropy_mode: EntropyMode::Deterministic,
            break_requested: false,
            loop_depth: 0,
            loop_stack: Vec::new(),
            pc: 0,
            instructions: Vec::new(),
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
