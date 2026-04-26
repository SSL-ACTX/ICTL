use crate::frontend::ast::{
    Capability, EntropyMode, Expression, MergeResolution, ParamMode,
    ResolutionStrategy, SpeculationCommitMode, Statement, TimeCoordinate,
};
use crate::runtime::gc::GarbageCollector;
use crate::runtime::memory::{
    Arena, EntropicState, MemoryError, Payload, PendingPromise,
};
use crate::runtime::vm::error::TemporalError;
use crate::runtime::vm::state::{
    AnchorPoint, Routine, SpeculationContext, Timeline, Vm,
};
use std::collections::{HashMap, VecDeque};

fn resolve_pending_payload(promise: &PendingPromise) -> Payload {
    if promise.capability == "System.NetworkFetch" {
        if let Some(url) = promise.params.get("url") {
            return Payload::String(format!("fetched:{}", url));
        }
    }

    if let Some(value) = promise.params.get("value") {
        return Payload::String(value.clone());
    }

    Payload::String("pending".to_string())
}

pub(crate) fn execute_statement_inner(
    vm: &mut Vm,
    branch_id: &str,
    stmt: &crate::frontend::ast::SpannedStatement,
) -> Result<(), TemporalError> {
    match &stmt.stmt {
        Statement::RelativisticBlock { time, body } => {
            let target_branch = match time {
                TimeCoordinate::Branch(id) => id.clone(),
                _ => branch_id.to_string(),
            };

            for inner_stmt in body {
                vm.execute_statement(&target_branch, inner_stmt)?;
            }
        }
        Statement::FieldUpdate {
            target,
            field,
            value,
        } => {
            let new_val = vm.evaluate_expression(branch_id, value)?;
            vm.update_nested_field(branch_id, target, field, new_val)?;
        }
        Statement::Watchdog {
            target,
            timeout_ms,
            recovery,
        } => {
            let should_bite = if let Ok(branch) = vm.get_branch_mut(target) {
                branch.local_clock > *timeout_ms
            } else {
                false
            };

            if should_bite {
                // Phase 13: Instead of deleting, we trigger recovery logic.
                // The recovery logic may use AcausalReset to fix the branch.
                for recovery_stmt in recovery {
                    vm.execute_statement(branch_id, recovery_stmt)?;
                }
                return Err(TemporalError::WatchdogBite(
                    target.clone(),
                    *timeout_ms,
                ));
            }
        }
        Statement::Speculate {
            max_ms,
            body,
            fallback,
        } => {
            let original_branch = vm.get_branch_mut(branch_id)?.clone();
            let history_start_index = vm.causal_history.len();

            let fallback_cost =
                vm.estimate_block_cost(fallback.as_ref().unwrap_or(&Vec::new()));

            let mut speculative_error: Option<TemporalError> = None;

            vm.speculation_stack.push(SpeculationContext::default());
            vm.set_branch_state(branch_id, original_branch.clone());

            for stmt in body {
                if let Err(err) = vm.execute_statement(branch_id, stmt) {
                    speculative_error = Some(err);
                    break;
                }

                let current_clock = vm.get_branch_mut(branch_id)?.local_clock;
                if current_clock.saturating_sub(original_branch.local_clock)
                    > *max_ms
                {
                    speculative_error = Some(TemporalError::WatchdogBite(
                        branch_id.to_string(),
                        *max_ms,
                    ));
                    break;
                }
            }

            let speculative_branch_snapshot = vm.get_branch_mut(branch_id)?.clone();

            let speculation_context = vm
                .speculation_stack
                .pop()
                .expect("speculation stack underflow");

            let commit_valid = speculative_error.is_none()
                && speculation_context.commit_executed
                && !speculation_context.collapse_happened;

            // Restore base state before applying either commit or fallback
            vm.set_branch_state(branch_id, original_branch.clone());

            if commit_valid {
                match vm.speculative_commit_mode {
                    SpeculationCommitMode::Full => {
                        vm.set_branch_state(branch_id, speculative_branch_snapshot);
                    }
                    SpeculationCommitMode::Selective => {
                        let branch = vm.get_branch_mut(branch_id)?;
                        for var in speculation_context.commit_vars.iter() {
                            if let Some(payload) =
                                speculative_branch_snapshot.arena.peek(var)
                            {
                                branch.arena.insert(
                                    var.clone(),
                                    EntropicState::Valid(payload),
                                )?;
                            } else {
                                branch.arena.set_consumed(var)?;
                            }
                        }
                    }
                }
                let branch = vm.get_branch_mut(branch_id)?;
                branch.commit_horizon_passed = true;
            } else {
                // Speculation failed/collapsed: Causal Rollback
                vm.causal_rollback(branch_id, history_start_index)?;
                vm.causal_history.truncate(history_start_index);

                if let Some(fallback_body) = fallback {
                    for stmt in fallback_body {
                        vm.execute_statement(branch_id, stmt)?;
                    }
                }
            }

            let branch = vm.get_branch_mut(branch_id)?;
            let target_clock =
                original_branch.local_clock + 1 + *max_ms + fallback_cost;
            if branch.local_clock < target_clock {
                let padding = target_clock - branch.local_clock;
                branch.local_clock += padding;
                branch.consume_budget(padding)?;
            }
        }
        Statement::Collapse => {
            if let Some(ctx) = vm.speculation_stack.last_mut() {
                ctx.collapse_happened = true;
            }
            return Err(TemporalError::SpeculationCollapsed);
        }
        Statement::Break => {
            let branch = vm.get_branch_mut(branch_id)?;
            if branch.loop_depth == 0 {
                return Err(TemporalError::InvalidBreak);
            }
            branch.break_requested = true;
        }
        Statement::SpeculationMode(mode) => {
            vm.speculative_commit_mode = *mode;
        }
        Statement::TypeDecl { .. } => {
            // Type declarations are a compile-time construct only.
        }
        Statement::AcausalReset {
            target,
            anchor_name,
        } => {
            // PHASE 13: Time-Loop Logic
            // We reach into a target branch and reset its state to a previous anchor.
            let anchor = {
                let target_branch = vm.get_branch_mut(target)?;
                target_branch
                    .anchors
                    .get(anchor_name)
                    .ok_or_else(|| {
                        TemporalError::AnchorNotFound(anchor_name.clone())
                    })?
                    .clone()
            };

            // Causal rollback for the target branch
            vm.causal_rollback(target, anchor.history_index)?;

            let target_branch = vm.get_branch_mut(target)?;
            target_branch.arena = anchor.arena_snapshot;
            target_branch.local_clock = anchor.clock_snapshot;
            target_branch.cpu_budget_ms = anchor.cpu_budget_snapshot;
            target_branch.resource_budgets = anchor.resource_budgets_snapshot;
            target_branch.commit_horizon_passed = false;
        }
        Statement::Inspect { target: _, body } => {
            let original_state = vm.get_branch_mut(branch_id)?.clone();
            for st in body {
                vm.execute_statement(branch_id, st)?;
            }
            vm.set_branch_state(branch_id, original_state);
        }
        Statement::Await(target) => {
            let branch = vm.get_branch_mut(branch_id)?;
            let status = branch
                .arena
                .bindings
                .get(target)
                .cloned()
                .unwrap_or(EntropicState::Consumed);

            match status {
                EntropicState::Pending(promise) => {
                    let current_time = branch.local_clock;
                    if current_time < promise.ready_at {
                        let delay = promise.ready_at - current_time;
                        branch.local_clock = promise.ready_at;
                        branch.consume_budget(delay)?;
                    }

                    if branch.local_clock <= promise.deadline_at {
                        let resolved = resolve_pending_payload(&promise);
                        branch.arena.insert(
                            target.clone(),
                            EntropicState::Valid(resolved),
                        )?;
                    } else {
                        branch.arena.set_consumed(target)?;
                    }
                }
                EntropicState::Valid(_) => { /* already available */ }
                EntropicState::Decayed(_) | EntropicState::Consumed => {
                    // Cannot await consumed/decayed value; no-op for now.
                }
            }
        }
        Statement::Isolate(block) => {
            let (capabilities, cpu_req) = {
                let branch = vm.get_branch_mut(branch_id)?;
                if let Some(limit_bytes) = block.manifest.memory_budget_bytes {
                    branch.arena.capacity = limit_bytes;
                }
                if let Some(mode) = block.manifest.mode {
                    branch.entropy_mode = mode;
                }
                // Apply resource budgets
                for (res, amount) in &block.manifest.resource_budgets {
                    branch.resource_budgets.insert(res.clone(), *amount);
                }
                branch.manifest_stack.push(block.manifest.clone());
                (
                    block.manifest.capabilities.clone(),
                    block.manifest.cpu_budget_ms,
                )
            };

            for cap in &capabilities {
                vm.execute_capability(branch_id, cap)?;
            }

            if let Some(cpu) = cpu_req {
                let branch = vm.get_branch_mut(branch_id)?;
                if cpu > branch.cpu_budget_ms {
                    return Err(TemporalError::BudgetExhausted);
                }
                branch.cpu_budget_ms = cpu;
                branch.slice_ms = Some(cpu);
            }

            for inner_stmt in &block.body {
                vm.execute_statement(branch_id, inner_stmt)?;
            }

            let branch = vm.get_branch_mut(branch_id)?;
            branch.manifest_stack.pop();
        }
        Statement::Slice { milliseconds } => {
            let branch = vm.get_branch_mut(branch_id)?;
            branch.slice_ms = Some(*milliseconds);
            if *milliseconds < branch.cpu_budget_ms {
                // Preserve absolute CPU budget but mark fixed slice.
                branch.cpu_budget_ms = *milliseconds;
            }
        }
        Statement::LoopTick { body } => {
            let slice_ms = {
                let branch = vm.get_branch_mut(branch_id)?;
                branch
                    .slice_ms
                    .or_else(|| Some(branch.cpu_budget_ms))
                    .ok_or_else(|| TemporalError::InvalidLoopBudget)?
            };

            {
                let branch = vm.get_branch_mut(branch_id)?;
                branch.loop_depth += 1;
            }

            let mut iterations = 0;

            loop {
                let iter_start = vm.get_branch_mut(branch_id)?.local_clock;
                let mut broke = false;

                for stmt in body {
                    vm.execute_statement(branch_id, stmt)?;
                    if vm.get_branch_mut(branch_id)?.break_requested {
                        broke = true;
                        break;
                    }
                }

                let branch = vm.get_branch_mut(branch_id)?;
                let body_cost = branch.local_clock.saturating_sub(iter_start);

                if body_cost > slice_ms {
                    return Err(TemporalError::WatchdogBite(
                        branch_id.to_string(),
                        slice_ms,
                    ));
                }

                let pad = slice_ms.saturating_sub(body_cost);
                let branch = vm.get_branch_mut(branch_id)?;
                branch.local_clock += pad;

                vm.commit_tick_buffers();

                if broke {
                    let branch = vm.get_branch_mut(branch_id)?;
                    branch.break_requested = false;
                    break;
                }

                iterations += 1;
                if iterations > 1000 {
                    return Err(TemporalError::WatchdogBite(
                        branch_id.to_string(),
                        slice_ms,
                    ));
                }
            }

            let branch = vm.get_branch_mut(branch_id)?;
            if branch.loop_depth > 0 {
                branch.loop_depth -= 1;
            }
        }
        Statement::RoutineDef {
            name,
            params,
            return_type,
            taking_ms,
            body,
        } => {
            let final_taking_ms =
                taking_ms.unwrap_or_else(|| vm.estimate_block_cost(body));
            vm.routines.insert(
                name.clone(),
                Routine {
                    params: params
                        .iter()
                        .map(|p| {
                            (
                                p.mode.clone(),
                                p.name.clone(),
                                p.typ
                                    .as_ref()
                                    .map(|t| {
                                        crate::analysis::types::Type::from_typename(
                                            t,
                                        )
                                    })
                                    .unwrap_or(
                                        crate::analysis::types::Type::Unknown,
                                    ),
                            )
                        })
                        .collect(),
                    return_type: return_type
                        .as_ref()
                        .map(|t| crate::analysis::types::Type::from_typename(t))
                        .unwrap_or(crate::analysis::types::Type::Unknown),
                    taking_ms: Some(final_taking_ms),
                    body: body.clone(),
                },
            );
        }
        Statement::Capability(cap) => {
            vm.execute_capability(branch_id, cap)?;
        }
        Statement::Print(expr) => {
            let payload = vm.evaluate_expression(branch_id, expr)?;
            let message = payload.to_string();
            let cap = Capability {
                path: "System.Log".to_string(),
                parameters: [("message".to_string(), message)].into(),
            };

            if vm.capability_handlers.contains_key("System.Log") {
                vm.execute_capability(branch_id, &cap)?;
            } else {
                // Fallback for host-side debugging outside strict capability isolation.
                println!("[ictl] {}", payload);
            }
        }
        Statement::Debug(expr) => {
            let payload = vm.evaluate_expression_nonconsuming(branch_id, expr)?;
            let message = payload.to_string();
            let cap = Capability {
                path: "System.Log".to_string(),
                parameters: [("message".to_string(), message)].into(),
            };

            if vm.capability_handlers.contains_key("System.Log") {
                vm.execute_capability(branch_id, &cap)?;
            } else {
                println!("[ictl-debug] {}", payload);
            }
        }
        Statement::Assignment { target, expr, .. } => {
            if let Expression::Deferred {
                capability,
                params,
                deadline_ms,
            } = expr
            {
                let branch = vm.get_branch_mut(branch_id)?;
                let requested_at = branch.local_clock;
                let deadline_at = requested_at + deadline_ms;
                let latency = params
                    .get("latency")
                    .and_then(|s| s.parse::<u64>().ok())
                    .unwrap_or_else(|| (deadline_ms / 2).max(1));
                let ready_at = requested_at + latency;

                let request = PendingPromise {
                    capability: capability.clone(),
                    params: params.clone(),
                    requested_at,
                    ready_at,
                    deadline_at,
                };

                branch
                    .arena
                    .insert(target.clone(), EntropicState::Pending(request))?;

                if let Some(ctx) = vm.speculation_stack.last_mut() {
                    if ctx.in_commit_block {
                        ctx.commit_vars.insert(target.clone());
                    }
                }
            } else {
                let payload = vm.evaluate_expression(branch_id, expr)?;
                if let Some(ctx) = vm.speculation_stack.last_mut() {
                    if ctx.in_commit_block {
                        ctx.commit_vars.insert(target.clone());
                    }
                }
                let branch = vm.get_branch_mut(branch_id)?;
                branch
                    .arena
                    .insert(target.clone(), EntropicState::Valid(payload))?;
            }
        }
        Statement::Split { parent, branches } => {
            let branches_str: Vec<&str> =
                branches.iter().map(|s| s.as_str()).collect();
            vm.split_timeline(parent, branches_str)?;
        }
        Statement::Merge {
            branches,
            target,
            resolutions,
        } => {
            let branches_str: Vec<&str> =
                branches.iter().map(|s| s.as_str()).collect();
            vm.merge_timelines(branches_str, target, resolutions)?;
        }
        Statement::Anchor(name) => {
            let history_index = vm.causal_history.len();
            let branch = vm.get_branch_mut(branch_id)?;
            let snapshot = AnchorPoint {
                name: name.clone(),
                clock_snapshot: branch.local_clock,
                arena_snapshot: branch.arena.clone(),
                cpu_budget_snapshot: branch.cpu_budget_ms,
                resource_budgets_snapshot: branch.resource_budgets.clone(),
                history_index,
            };
            branch.anchors.insert(name.clone(), snapshot);
        }
        Statement::Rewind(name) => {
            let branch = vm.get_branch_mut(branch_id)?;
            if branch.entropy_mode == EntropyMode::Chaos {
                return Err(TemporalError::RewindDisabledInChaos);
            }
            let anchor = branch
                .anchors
                .get(name)
                .ok_or_else(|| {
                    if branch.commit_horizon_passed {
                        TemporalError::CommitHorizonViolation
                    } else {
                        TemporalError::AnchorNotFound(name.clone())
                    }
                })?
                .clone();

            // Causal Validation and Reversion
            vm.causal_rollback(branch_id, anchor.history_index)?;

            let branch = vm.get_branch_mut(branch_id)?;
            branch.arena = anchor.arena_snapshot;
            branch.local_clock = anchor.clock_snapshot;
            branch.cpu_budget_ms = anchor.cpu_budget_snapshot;
            branch.resource_budgets = anchor.resource_budgets_snapshot;
        }
        Statement::Commit(body) => {
            if let Some(ctx) = vm.speculation_stack.last_mut() {
                ctx.commit_executed = true;
                ctx.in_commit_block = true;
            }

            for inner_stmt in body {
                vm.execute_statement(branch_id, inner_stmt)?;
            }

            if let Some(ctx) = vm.speculation_stack.last_mut() {
                ctx.in_commit_block = false;
            }

            let branch = vm.get_branch_mut(branch_id)?;
            branch.commit_horizon_passed = true;
            crate::runtime::gc::GarbageCollector::collect_commit_anchors(branch);
            branch.arena.compact_consumed();
        }
        Statement::Send {
            value_id,
            target_branch,
        } => {
            let payload = {
                let branch = vm.get_branch_mut(branch_id)?;
                branch.arena.consume(value_id)?
            };
            vm.propagate_entanglement(branch_id, value_id)?;

            let payload_id = vm.next_payload_id;
            vm.next_payload_id += 1;
            let message = crate::runtime::vm::state::Message {
                id: payload_id,
                sender: branch_id.to_string(),
                payload: payload.clone(),
            };

            // Record the event
            vm.causal_history.push(
                crate::runtime::vm::state::CausalEvent::InterBranchMove {
                    source_branch: branch_id.to_string(),
                    target_branch: target_branch.clone(),
                    var_name: value_id.clone(),
                    message,
                },
            );

            let target = vm.get_branch_mut(target_branch)?;
            target
                .arena
                .insert(value_id.clone(), EntropicState::Valid(payload))?;
        }
        Statement::ChannelOpen { name, capacity } => {
            vm.channels
                .insert(name.clone(), VecDeque::with_capacity(*capacity));
            vm.pending_channels
                .insert(name.clone(), VecDeque::with_capacity(*capacity));
        }
        Statement::ChannelSend { chan_id, value_id } => {
            let payload = {
                let branch = vm.get_branch_mut(branch_id)?;
                branch.arena.consume(value_id)?
            };
            vm.propagate_entanglement(branch_id, value_id)?;
            let isochronous = vm.get_branch_mut(branch_id)?.slice_ms.is_some();

            let payload_id = vm.next_payload_id;
            vm.next_payload_id += 1;

            let message = crate::runtime::vm::state::Message {
                id: payload_id,
                sender: branch_id.to_string(),
                payload,
            };

            // Record the event
            vm.causal_history.push(
                crate::runtime::vm::state::CausalEvent::ChannelSend {
                    branch_id: branch_id.to_string(),
                    channel_id: chan_id.clone(),
                    payload_id,
                },
            );

            if isochronous {
                let pending =
                    vm.pending_channels.get_mut(chan_id).ok_or_else(|| {
                        TemporalError::ChannelFault(format!(
                            "Channel not found: {}",
                            chan_id
                        ))
                    })?;
                pending.push_back(message);
            } else {
                let chan = vm.channels.get_mut(chan_id).ok_or_else(|| {
                    TemporalError::ChannelFault(format!(
                        "Channel not found: {}",
                        chan_id
                    ))
                })?;
                chan.push_back(message);
            }
        }
        Statement::Select {
            max_ms: _,
            cases,
            timeout,
            reconcile: _,
        } => {
            let original_clock = {
                let branch = vm.get_branch_mut(branch_id)?;
                branch.local_clock
            };

            let mut selected_case = None;
            for case in cases {
                if let Expression::ChannelReceive(chan_id) = &case.source {
                    if let Some(chan) = vm.channels.get(chan_id) {
                        if !chan.is_empty() {
                            selected_case = Some(case);
                            break;
                        }
                    }
                }
            }

            if let Some(case) = selected_case {
                if let Expression::ChannelReceive(chan_id) = &case.source {
                    let message = vm
                        .channels
                        .get_mut(chan_id)
                        .and_then(|q| q.pop_front())
                        .ok_or_else(|| {
                            TemporalError::ChannelFault(format!(
                                "Channel empty: {}",
                                chan_id
                            ))
                        })?;

                    // Record the event
                    vm.causal_history.push(
                        crate::runtime::vm::state::CausalEvent::ChannelRecv {
                            branch_id: branch_id.to_string(),
                            channel_id: chan_id.clone(),
                            message: message.clone(),
                        },
                    );

                    let branch = vm.get_branch_mut(branch_id)?;
                    branch.arena.insert(
                        case.binding.clone(),
                        EntropicState::Valid(message.payload),
                    )?;
                }
                for stmt in &case.body {
                    vm.execute_statement(branch_id, stmt)?;
                }
                // case binding is local to the select block
                let branch = vm.get_branch_mut(branch_id)?;
                branch.arena.bindings.remove(&case.binding);
            } else if let Some(timeout_body) = timeout {
                for stmt in timeout_body {
                    vm.execute_statement(branch_id, stmt)?;
                }
            }

            let case_max_cost = cases
                .iter()
                .map(|c| vm.estimate_block_cost(&c.body))
                .max()
                .unwrap_or(0);
            let timeout_cost = timeout
                .as_ref()
                .map(|b| vm.estimate_block_cost(b))
                .unwrap_or(0);

            let target_clock = original_clock + 1 + case_max_cost.max(timeout_cost);
            let branch = vm.get_branch_mut(branch_id)?;
            if branch.local_clock < target_clock {
                let padding = target_clock - branch.local_clock;
                branch.local_clock += padding;
                branch.consume_budget(padding)?;
            }
        }
        Statement::MatchEntropy {
            target,
            valid_branch,
            decayed_branch,
            pending_branch,
            consumed_branch,
        } => {
            let status = match target {
                Expression::Identifier(name) => {
                    let branch = vm.get_branch_mut(branch_id)?;
                    branch
                        .arena
                        .bindings
                        .get(name)
                        .cloned()
                        .unwrap_or(EntropicState::Consumed)
                }
                Expression::IndexAccess {
                    target: parent_expr,
                    index,
                } => {
                    let index_payload =
                        vm.evaluate_expression_nonconsuming(branch_id, index)?;
                    let index_str = match index_payload {
                        Payload::String(s) => s,
                        Payload::Integer(i) => i.to_string(),
                        _ => {
                            return Err(TemporalError::EvalError(
                                "Index must be string or integer".into(),
                            ))
                        }
                    };
                    let parent_payload =
                        vm.evaluate_expression_nonconsuming(branch_id, parent_expr)?;
                    match parent_payload {
                        Payload::Struct(fields) | Payload::Topology(fields) => {
                            fields
                                .get(&index_str)
                                .cloned()
                                .unwrap_or(EntropicState::Consumed)
                        }
                        _ => EntropicState::Consumed,
                    }
                }
                _ => EntropicState::Consumed,
            };

            match status {
                EntropicState::Valid(_) | EntropicState::Decayed(_) => {
                    let selected = match status {
                        EntropicState::Valid(_) => valid_branch.as_ref(),
                        EntropicState::Decayed(_) => decayed_branch.as_ref(),
                        _ => None,
                    };
                    if let Some((binding, body)) = selected {
                        let consumed_state =
                            vm.evaluate_entropic_state(branch_id, target)?;
                        let branch = vm.get_branch_mut(branch_id)?;
                        branch.arena.insert(binding.clone(), consumed_state)?;
                        for stmt in body {
                            vm.execute_statement(branch_id, stmt)?;
                        }
                    }
                }
                EntropicState::Pending(_) => {
                    if let Some(body) = pending_branch {
                        for stmt in body {
                            vm.execute_statement(branch_id, stmt)?;
                        }
                    }
                }
                EntropicState::Consumed => {
                    if let Some(body) = consumed_branch {
                        for stmt in body {
                            vm.execute_statement(branch_id, stmt)?;
                        }
                    }
                }
            }
        }
        Statement::Entangle { variables } => {
            let mut group = std::collections::HashSet::new();
            for var in variables {
                group.insert((branch_id.to_string(), var.clone()));
            }
            vm.entanglements.push(group);
        }
        Statement::If {
            condition,
            then_branch,
            else_branch,
            reconcile,
        } => {
            let cond_payload = vm.evaluate_expression(branch_id, condition)?;
            let cond_true = match cond_payload {
                Payload::Bool(b) => b,
                _ => {
                    return Err(TemporalError::EvalError(
                        "if condition must be bool".into(),
                    ))
                }
            };

            let then_cost = vm.estimate_block_cost(then_branch);
            let else_cost =
                vm.estimate_block_cost(else_branch.as_ref().unwrap_or(&Vec::new()));
            let max_cost = then_cost.max(else_cost) + 1; // 1ms overhead

            // clone environment for speculative branch execution
            let original_channels = vm.channels.clone();
            let original_branch = vm.get_branch_mut(branch_id)?.clone();

            let then_state = vm.simulate_branch(branch_id, then_branch)?;
            vm.channels = original_channels.clone();
            let else_state = vm.simulate_branch(
                branch_id,
                else_branch.as_ref().unwrap_or(&Vec::new()),
            )?;
            vm.channels = original_channels.clone();

            let mut final_state = if cond_true {
                then_state.clone()
            } else {
                else_state.clone()
            };

            if let Some(reconcile_rules) = reconcile {
                for (var, strat) in &reconcile_rules.rules {
                    let existing = then_state
                        .arena
                        .bindings
                        .get(var)
                        .cloned()
                        .unwrap_or(EntropicState::Consumed);
                    let incoming = else_state
                        .arena
                        .bindings
                        .get(var)
                        .cloned()
                        .unwrap_or(EntropicState::Consumed);

                    let (resolved, rev) = vm.resolve_entropic_conflict(
                        var,
                        &existing,
                        &incoming,
                        strat,
                        if cond_true { "if" } else { "else" },
                    );

                    final_state.arena.insert(var.clone(), resolved)?;

                    if let Some(reversion) = rev {
                        let anchor = {
                            let target_branch =
                                vm.get_branch_mut(&reversion.branch)?;
                            target_branch
                                .anchors
                                .get(&reversion.anchor)
                                .ok_or_else(|| {
                                    TemporalError::AnchorNotFound(
                                        reversion.anchor.clone(),
                                    )
                                })?
                                .clone()
                        };
                        vm.causal_rollback(&reversion.branch, anchor.history_index)?;
                        let target_branch = vm.get_branch_mut(&reversion.branch)?;
                        target_branch.arena = anchor.arena_snapshot;
                        target_branch.local_clock = anchor.clock_snapshot;
                        target_branch.cpu_budget_ms = anchor.cpu_budget_snapshot;
                        target_branch.resource_budgets =
                            anchor.resource_budgets_snapshot;
                        target_branch.commit_horizon_passed = false;
                        return Ok(());
                    }
                }
            }

            vm.set_branch_state(branch_id, final_state);

            let branch = vm.get_branch_mut(branch_id)?;
            let run_cost = branch.local_clock - original_branch.local_clock;
            if run_cost < max_cost {
                let padding = max_cost - run_cost;
                branch.local_clock += padding;
                branch.consume_budget(padding)?;
            }
        }
        Statement::For {
            item_name,
            mode,
            source,
            body,
            pacing_ms,
            max_ms,
        } => {
            let source_payload = {
                let branch = vm.get_branch_mut(branch_id)?;
                match mode {
                    crate::frontend::ast::ForMode::Consume => {
                        let p = branch.arena.consume(source)?;
                        vm.propagate_entanglement(branch_id, source)?;
                        p
                    }
                    crate::frontend::ast::ForMode::Clone => branch
                        .arena
                        .peek(source)
                        .ok_or(MemoryError::AlreadyConsumed)?,
                }
            };

            let elements = match source_payload {
                Payload::Array(vec) => vec,
                Payload::Struct(fields) => {
                    let mut items: Vec<Payload> = Vec::new();
                    for (key, state) in fields {
                        if let EntropicState::Valid(value) = state {
                            let mut map = std::collections::HashMap::new();
                            map.insert(
                                "key".to_string(),
                                EntropicState::Valid(Payload::String(key)),
                            );
                            map.insert(
                                "value".to_string(),
                                EntropicState::Valid(value),
                            );
                            items.push(Payload::Struct(map));
                        }
                    }
                    items
                }
                _ => {
                    return Err(TemporalError::EvalError(
                        "for-source must be array or struct".into(),
                    ))
                }
            };

            let mut elapsed = 0;
            let max_allowed = max_ms.unwrap_or(u64::MAX);

            for item_value in elements.into_iter() {
                if elapsed >= max_allowed {
                    break;
                }

                {
                    let branch = vm.get_branch_mut(branch_id)?;
                    branch.arena.insert(
                        item_name.clone(),
                        EntropicState::Valid(item_value),
                    )?;
                }

                let iteration_start = vm.get_branch_mut(branch_id)?.local_clock;
                for stmt in body {
                    vm.execute_statement(branch_id, stmt)?;
                    if vm.get_branch_mut(branch_id)?.break_requested {
                        let branch = vm.get_branch_mut(branch_id)?;
                        branch.break_requested = false;
                        break;
                    }
                }

                let body_cost =
                    vm.get_branch_mut(branch_id)?.local_clock - iteration_start;
                let paced = pacing_ms.unwrap_or(body_cost);

                if body_cost > paced {
                    return Err(TemporalError::PacingViolation);
                }

                let pad = paced - body_cost;
                if pad > 0 {
                    let branch = vm.get_branch_mut(branch_id)?;
                    branch.local_clock += pad;
                    branch.consume_budget(pad)?;
                }

                elapsed += paced;
            }

            if let Some(max) = max_ms {
                let branch = vm.get_branch_mut(branch_id)?;
                if branch.local_clock < *max {
                    let padding = *max - branch.local_clock;
                    branch.local_clock += padding;
                    branch.consume_budget(padding)?;
                }
            }
        }
        Statement::SplitMap {
            item_name,
            mode,
            source,
            body,
            reconcile,
        } => {
            let source_payload = {
                let branch = vm.get_branch_mut(branch_id)?;
                match mode {
                    crate::frontend::ast::ForMode::Consume => {
                        let p = branch.arena.consume(source)?;
                        vm.propagate_entanglement(branch_id, source)?;
                        p
                    }
                    crate::frontend::ast::ForMode::Clone => branch
                        .arena
                        .peek(source)
                        .ok_or(MemoryError::AlreadyConsumed)?,
                }
            };
            let elements = match source_payload {
                Payload::Array(vec) => vec,
                _ => {
                    return Err(TemporalError::EvalError(
                        "split_map source must be array".into(),
                    ))
                }
            };

            let mut results: Vec<Payload> = Vec::new();

            for item_value in elements.into_iter() {
                let child_name = format!("splitmap_{}", results.len());
                let child_snapshot = vm.get_branch_mut(branch_id)?.clone();

                vm.active_branches
                    .insert(child_name.clone(), child_snapshot);
                {
                    let child_branch = vm.get_branch_mut(&child_name)?;
                    child_branch.arena.insert(
                        item_name.clone(),
                        EntropicState::Valid(item_value),
                    )?;
                }

                for stmt in body {
                    vm.execute_statement(&child_name, stmt)?;
                }

                let yielded = vm
                    .get_branch_mut(&child_name)?
                    .arena
                    .peek("yielded")
                    .map(|p| p.clone());
                if let Some(Payload::Array(arr)) = yielded {
                    results.extend(arr);
                }

                vm.terminate_branch(&child_name)?;
            }

            let branch = vm.get_branch_mut(branch_id)?;
            branch.arena.insert(
                "splitmap_results".into(),
                EntropicState::Valid(Payload::Array(results)),
            )?;

            if let Some(_resolver) = reconcile {
                // placeholder: resolver semantics can be finalized later
            }
        }
        Statement::Yield(item) => {
            let branch = vm.get_branch_mut(branch_id)?;
            let value = branch.arena.consume(item)?;
            match branch.arena.peek("yielded") {
                Some(Payload::Array(mut existing)) => {
                    existing.push(value);
                    branch.arena.insert(
                        "yielded".into(),
                        EntropicState::Valid(Payload::Array(existing)),
                    )?;
                }
                _ => {
                    branch.arena.insert(
                        "yielded".into(),
                        EntropicState::Valid(Payload::Array(vec![value])),
                    )?;
                }
            }
        }
        Statement::Loop { max_ms, body } => {
            let branch = vm.get_branch_mut(branch_id)?;
            if *max_ms == 0 {
                return Err(TemporalError::InvalidLoopBudget);
            }
            branch.loop_depth += 1;
            let loop_start = branch.local_clock;
            let mut iterations = 0;

            while vm.get_branch_mut(branch_id)?.local_clock - loop_start < *max_ms {
                iterations += 1;
                if iterations > 1000 {
                    return Err(TemporalError::WatchdogBite(
                        branch_id.to_string(),
                        *max_ms,
                    ));
                }

                let iter_start = vm.get_branch_mut(branch_id)?.local_clock;
                for stmt in body {
                    vm.execute_statement(branch_id, stmt)?;
                    if vm.get_branch_mut(branch_id)?.break_requested {
                        break;
                    }
                }

                if vm.get_branch_mut(branch_id)?.break_requested {
                    let branch = vm.get_branch_mut(branch_id)?;
                    branch.break_requested = false;
                    break;
                }

                if vm.get_branch_mut(branch_id)?.local_clock == iter_start {
                    break;
                }
            }

            let branch = vm.get_branch_mut(branch_id)?;
            let target_clock = loop_start + *max_ms;
            if branch.local_clock < target_clock {
                let padding = target_clock - branch.local_clock;
                branch.local_clock += padding;
                branch.consume_budget(padding)?;
            }

            if branch.loop_depth > 0 {
                branch.loop_depth -= 1;
            }
        }
        Statement::Expression(expr) => {
            vm.evaluate_expression(branch_id, expr)?;
        }
        Statement::NetworkRequest { .. } => {
            let branch = vm.get_branch_mut(branch_id)?;
            branch.local_clock += 5;
            branch.consume_budget(5)?;
        }
    }
    Ok(())
}
