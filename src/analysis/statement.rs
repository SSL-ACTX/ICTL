use crate::analysis::analyzer::{
    BranchState, EntropicAnalyzer, RoutineInfo, SemanticError, SemanticErrorKind,
};
use crate::analysis::expression::{
    analyze_expression, analyze_expression_nonconsuming, estimate_expression_cost,
};
use crate::analysis::types::Type;
use crate::frontend::ast::*;
use std::collections::HashSet;

pub(crate) fn analyze_statement(
    analyzer: &mut EntropicAnalyzer,
    stmt: &SpannedStatement,
) -> Result<(), SemanticError> {
    match &stmt.stmt {
        Statement::RelativisticBlock { time, body } => {
            let old_branch = analyzer.current_branch.clone();
            if let TimeCoordinate::Branch(id) = time {
                analyzer.current_branch = id.clone();
            }
            for inner_stmt in body {
                analyze_statement(analyzer, inner_stmt)?;
            }
            analyzer.current_branch = old_branch;
        }
        Statement::FieldUpdate { target, value, .. } => {
            if let Expression::Identifier(name) = target {
                let branch = analyzer
                    .branch_contexts
                    .get(&analyzer.current_branch)
                    .unwrap();
                if branch.consumed.contains(name) {
                    return Err(analyzer.annotate(
                        SemanticErrorKind::CrossBranchViolation(name.clone()),
                    ));
                }
            } else {
                analyze_expression(analyzer, target)?;
            }
            analyze_expression(analyzer, value)?;
        }
        Statement::Watchdog { recovery, .. } => {
            for inner_stmt in recovery {
                analyze_statement(analyzer, inner_stmt)?;
            }
        }
        Statement::If {
            condition,
            then_branch,
            else_branch,
            reconcile,
        } => {
            let condition_type = crate::analysis::expression::infer_expression_type(
                analyzer, condition,
            )?;
            if condition_type != crate::analysis::types::Type::Bool {
                return Err(analyzer.annotate(SemanticErrorKind::TypeMismatch(
                    format!("if condition must be bool, got {:?}", condition_type),
                )));
            }
            analyze_expression(analyzer, condition)?;

            let original_state = analyzer
                .branch_contexts
                .get(&analyzer.current_branch)
                .cloned()
                .unwrap_or_default();

            let then_state = original_state.clone();
            let mut then_contexts = analyzer.branch_contexts.clone();
            then_contexts
                .insert(analyzer.current_branch.clone(), then_state.clone());
            let previous_contexts =
                std::mem::replace(&mut analyzer.branch_contexts, then_contexts);

            for inner_stmt in then_branch {
                analyze_statement(analyzer, inner_stmt)?;
            }
            let then_end_state = analyzer
                .branch_contexts
                .get(&analyzer.current_branch)
                .cloned()
                .unwrap_or_default();

            analyzer.branch_contexts = previous_contexts.clone();
            let else_state = original_state.clone();
            let mut else_contexts = analyzer.branch_contexts.clone();
            else_contexts
                .insert(analyzer.current_branch.clone(), else_state.clone());
            analyzer.branch_contexts = else_contexts;

            if let Some(else_branch) = else_branch {
                for inner_stmt in else_branch {
                    analyze_statement(analyzer, inner_stmt)?;
                }
            }
            let else_end_state = analyzer
                .branch_contexts
                .get(&analyzer.current_branch)
                .cloned()
                .unwrap_or_default();

            let mut mismatch_vars = Vec::new();
            for name in then_end_state
                .consumed
                .union(&else_end_state.consumed)
                .cloned()
            {
                let in_then = then_end_state.consumed.contains(&name);
                let in_else = else_end_state.consumed.contains(&name);
                if in_then != in_else {
                    mismatch_vars.push(name);
                }
            }

            if !mismatch_vars.is_empty() {
                if let Some(reconcile_rules) = reconcile {
                    if !reconcile_rules.auto {
                        for name in &mismatch_vars {
                            if !reconcile_rules.rules.contains_key(name) {
                                return Err(analyzer.annotate(
                                    SemanticErrorKind::EntropyMismatch(
                                        mismatch_vars.join(", "),
                                    ),
                                ));
                            }
                        }
                    }
                } else {
                    return Err(analyzer.annotate(
                        SemanticErrorKind::EntropyMismatch(mismatch_vars.join(", ")),
                    ));
                }
            }

            let mut merged_types = then_end_state.types.clone();
            for (name, typ) in &else_end_state.types {
                merged_types
                    .entry(name.clone())
                    .and_modify(|existing| {
                        if existing != typ {
                            *existing = crate::analysis::types::Type::Unknown;
                        }
                    })
                    .or_insert(typ.clone());
            }

            let merged_state = BranchState {
                consumed: then_end_state
                    .consumed
                    .union(&else_end_state.consumed)
                    .cloned()
                    .collect(),
                decayed: then_end_state
                    .decayed
                    .union(&else_end_state.decayed)
                    .cloned()
                    .collect(),
                yields: then_end_state
                    .yields
                    .union(&else_end_state.yields)
                    .cloned()
                    .collect(),
                mutables: then_end_state
                    .mutables
                    .union(&else_end_state.mutables)
                    .cloned()
                    .collect(),
                types: merged_types,
                custom_types: then_end_state.custom_types.clone(),
            };

            analyzer.branch_contexts = previous_contexts;
            analyzer
                .branch_contexts
                .insert(analyzer.current_branch.clone(), merged_state);
        }
        Statement::Inspect { target: _, body } => {
            let snapshot = analyzer
                .branch_contexts
                .get(&analyzer.current_branch)
                .cloned()
                .unwrap_or_default();

            analyzer.inspection_depth += 1;
            for inner_stmt in body {
                analyze_statement(analyzer, inner_stmt)?;
            }
            analyzer.inspection_depth -= 1;

            analyzer
                .branch_contexts
                .insert(analyzer.current_branch.clone(), snapshot);
        }
        Statement::TypeDecl { name, fields } => {
            let mut schema = std::collections::HashMap::new();
            for (field_name, field_type) in fields {
                schema.insert(field_name.clone(), Type::from_typename(field_type));
            }
            let type_struct = Type::Struct(schema);
            analyzer.set_custom_type(name, type_struct);
        }
        Statement::Await(target) => {
            let state = analyzer
                .branch_contexts
                .get(&analyzer.current_branch)
                .unwrap();
            if state.consumed.contains(target) {
                return Err(analyzer
                    .annotate(SemanticErrorKind::UseAfterConsume(target.clone())));
            }
        }
        Statement::Slice { milliseconds } => {
            analyzer.current_slice_ms = Some(*milliseconds);
        }
        Statement::Loop { max_ms, body } => {
            if *max_ms == 0 {
                return Err(analyzer.annotate(SemanticErrorKind::InvalidLoopBudget));
            }
            for inner_stmt in body {
                analyze_statement(analyzer, inner_stmt)?;
            }
        }
        Statement::LoopTick { body } => {
            let slice_ms = analyzer.current_slice_ms.ok_or_else(|| {
                analyzer.annotate(SemanticErrorKind::TickLoopWithoutSlice)
            })?;

            let body_cost =
                crate::analysis::statement::estimate_block_cost(analyzer, body);
            if body_cost > slice_ms {
                return Err(analyzer.annotate(
                    SemanticErrorKind::TickLoopBudgetExceeded(body_cost, slice_ms),
                ));
            }

            let has_break = body
                .iter()
                .any(|inner_stmt| matches!(inner_stmt.stmt, Statement::Break));

            if !has_break {
                return Err(analyzer.annotate(SemanticErrorKind::TickLoopNeedsBreak));
            }

            for inner_stmt in body {
                analyze_statement(analyzer, inner_stmt)?;
            }
        }
        Statement::Isolate(block) => {
            let mut cap_set = HashSet::new();
            for cap in &block.manifest.capabilities {
                cap_set.insert(cap.path.clone());
            }
            analyzer.capability_stack.push(cap_set);

            let previous_slice = analyzer.current_slice_ms;
            analyzer.current_slice_ms = block.manifest.cpu_budget_ms;

            for inner_stmt in &block.body {
                analyze_statement(analyzer, inner_stmt)?;
            }

            analyzer.current_slice_ms = previous_slice;
            analyzer.capability_stack.pop();
        }
        Statement::RoutineDef {
            name,
            params,
            return_type,
            taking_ms,
            body,
        } => {
            if analyzer.routines.contains_key(name) {
                return Err(analyzer.annotate(SemanticErrorKind::EntropyMismatch(
                    format!("duplicate routine {}", name),
                )));
            }

            let mut routine_analyzer = EntropicAnalyzer::new();
            routine_analyzer.routines = analyzer.routines.clone();

            for stmt in body {
                match &stmt.stmt {
                    Statement::Split { .. }
                    | Statement::Merge { .. }
                    | Statement::RelativisticBlock { .. } => {
                        return Err(analyzer.annotate(SemanticErrorKind::EntropyMismatch(
                            "routines cannot contain split/merge/relativistic blocks".to_string(),
                        )));
                    }
                    _ => {}
                }
            }

            for param in params {
                let routine_state =
                    routine_analyzer.branch_contexts.get_mut("main").unwrap();
                routine_state.yields.insert(param.name.clone());
                let _ = routine_state;

                let param_type = param
                    .typ
                    .as_ref()
                    .map(|t| crate::analysis::types::Type::from_typename(t))
                    .unwrap_or(crate::analysis::types::Type::Unknown);
                routine_analyzer.set_variable_type(&param.name, param_type);
            }

            if let Some(rt) = return_type {
                // store a placeholder for return type in analyzer state if needed
                let routine_state =
                    routine_analyzer.branch_contexts.get_mut("main").unwrap();
                routine_state.yields.insert("<return>".to_string());
                let _ = routine_state;

                routine_analyzer.set_variable_type(
                    "<return>",
                    crate::analysis::types::Type::from_typename(&rt),
                );
            }

            for stmt in body {
                analyze_statement(&mut routine_analyzer, stmt)?;
            }

            let estimated_cost = estimate_block_cost(analyzer, body);
            let final_taking_ms = if let Some(ms) = *taking_ms {
                if estimated_cost > ms {
                    return Err(analyzer.annotate(
                        SemanticErrorKind::RoutineBudgetExceeded(
                            name.clone(),
                            ms,
                            estimated_cost,
                        ),
                    ));
                }
                ms
            } else {
                estimated_cost
            };

            let routine_params = params
                .iter()
                .map(|p| {
                    (
                        p.mode.clone(),
                        p.name.clone(),
                        p.typ
                            .as_ref()
                            .map(|t| crate::analysis::types::Type::from_typename(t))
                            .unwrap_or(crate::analysis::types::Type::Unknown),
                    )
                })
                .collect();
            let routine_info = RoutineInfo {
                params: routine_params,
                return_type: return_type
                    .as_ref()
                    .map(|t| crate::analysis::types::Type::from_typename(t))
                    .unwrap_or(crate::analysis::types::Type::Unknown),
                taking_ms: final_taking_ms,
            };

            analyzer.routines.insert(name.clone(), routine_info);
        }
        Statement::Assignment {
            target,
            mutable,
            var_type,
            expr,
        } => {
            let expr_type =
                crate::analysis::expression::infer_expression_type(analyzer, expr)?;
            analyze_expression(analyzer, expr)?;

            let state = analyzer
                .branch_contexts
                .get_mut(&analyzer.current_branch)
                .unwrap();
            if state.types.contains_key(target)
                && !state.mutables.contains(target)
                && !mutable
            {
                return Err(analyzer.annotate(SemanticErrorKind::TypeMismatch(
                    format!("cannot assign to immutable variable '{}'", target),
                )));
            }
            if *mutable {
                state.mutables.insert(target.clone());
            }

            let inferred_target_type = if let Some(expected_type) = var_type {
                crate::analysis::types::Type::from_typename(expected_type)
            } else {
                analyzer
                    .get_variable_type(target)
                    .unwrap_or(expr_type.clone())
            };

            let expected_type = analyzer.resolve_type(&inferred_target_type);
            let actual_type = analyzer.resolve_type(&expr_type);

            if !analyzer.types_compatible(&expected_type, &actual_type) {
                return Err(analyzer.annotate(SemanticErrorKind::TypeMismatch(
                    format!(
                        "variable '{}' assigned type {:?}, expected {:?}",
                        target, actual_type, expected_type
                    ),
                )));
            }

            if let Some(expected_type) = var_type {
                analyzer
                    .set_variable_type(target, Type::from_typename(expected_type));
            } else {
                analyzer.set_variable_type(target, expr_type);
            }

            let state = analyzer
                .branch_contexts
                .get_mut(&analyzer.current_branch)
                .unwrap();
            state.consumed.remove(target);
            state.yields.insert(target.clone());
        }
        Statement::Split { parent, branches } => {
            let parent_state = analyzer
                .branch_contexts
                .get(parent)
                .cloned()
                .unwrap_or_default();

            for branch in branches {
                analyzer.branch_contexts.insert(
                    branch.clone(),
                    BranchState {
                        consumed: parent_state.consumed.clone(),
                        decayed: parent_state.decayed.clone(),
                        yields: HashSet::new(),
                        mutables: parent_state.mutables.clone(),
                        types: parent_state.types.clone(),
                        custom_types: parent_state.custom_types.clone(),
                    },
                );
            }
            analyzer.mark_consumed(parent)?;
        }
        Statement::Merge {
            branches,
            target,
            resolutions,
        } => {
            let mut all_yields = HashSet::new();
            let mut collisions = HashSet::new();

            for branch_name in branches {
                let branch_state =
                    analyzer.branch_contexts.get(branch_name).ok_or_else(|| {
                        analyzer.annotate(SemanticErrorKind::CrossBranchViolation(
                            branch_name.clone(),
                        ))
                    })?;

                for y in &branch_state.yields {
                    if !all_yields.insert(y.clone()) {
                        collisions.insert(y.clone());
                    }
                }
            }

            for key in collisions {
                if !resolutions.rules.contains_key(&key) {
                    return Err(
                        analyzer.annotate(SemanticErrorKind::UnresolvedMerge(key))
                    );
                }
            }

            let target_state =
                analyzer.branch_contexts.entry(target.clone()).or_default();
            for y in all_yields {
                target_state.yields.insert(y.clone());
                target_state.consumed.remove(&y);
            }
        }
        Statement::Send { value_id, .. } => {
            analyzer.mark_consumed(value_id)?;
        }
        Statement::ChannelSend { value_id, .. } => {
            analyzer.mark_consumed(value_id)?;
        }
        Statement::Break => {}
        Statement::Select {
            max_ms: _,
            cases,
            timeout,
            reconcile,
        } => {
            let original_state = analyzer
                .branch_contexts
                .get(&analyzer.current_branch)
                .cloned()
                .unwrap_or_default();

            let mut branch_results = Vec::new();

            for case in cases {
                let case_type = crate::analysis::expression::infer_expression_type(
                    analyzer,
                    &case.source,
                )
                .unwrap_or(crate::analysis::types::Type::Unknown);
                analyze_expression(analyzer, &case.source)?;

                let saved_contexts = analyzer.branch_contexts.clone();
                let mut branch_contexts = analyzer.branch_contexts.clone();
                branch_contexts
                    .insert(analyzer.current_branch.clone(), original_state.clone());
                analyzer.branch_contexts = branch_contexts;
                analyzer.set_variable_type(&case.binding, case_type);

                for stmt in &case.body {
                    analyze_statement(analyzer, stmt)?;
                }

                let mut end_state = analyzer
                    .branch_contexts
                    .get(&analyzer.current_branch)
                    .cloned()
                    .unwrap_or_default();
                end_state.consumed.remove(&case.binding);
                end_state.yields.remove(&case.binding);
                branch_results.push(end_state);
                analyzer.branch_contexts = saved_contexts;
            }

            if let Some(timeout_body) = timeout {
                let saved_contexts = analyzer.branch_contexts.clone();
                let mut branch_contexts = analyzer.branch_contexts.clone();
                branch_contexts
                    .insert(analyzer.current_branch.clone(), original_state.clone());
                analyzer.branch_contexts = branch_contexts;

                for stmt in timeout_body {
                    analyze_statement(analyzer, stmt)?;
                }

                let end_state = analyzer
                    .branch_contexts
                    .get(&analyzer.current_branch)
                    .cloned()
                    .unwrap_or_default();
                branch_results.push(end_state);
                analyzer.branch_contexts = saved_contexts;
            }

            let merged_state = if branch_results.is_empty() {
                original_state.clone()
            } else {
                let mut merged = original_state.clone();
                for st in &branch_results {
                    merged.consumed.extend(st.consumed.clone().into_iter());
                    merged.yields.extend(st.yields.clone().into_iter());
                }
                merged
            };

            let all_vars: std::collections::HashSet<_> = branch_results
                .iter()
                .flat_map(|s| s.consumed.iter().cloned())
                .collect();

            let mut mismatch_vars = Vec::new();
            for var in all_vars {
                let in_some =
                    branch_results.iter().any(|s| s.consumed.contains(&var));
                let in_all =
                    branch_results.iter().all(|s| s.consumed.contains(&var));
                if in_some && !in_all {
                    mismatch_vars.push(var.clone());
                }
            }

            if !mismatch_vars.is_empty() && reconcile.is_none() {
                return Err(analyzer.annotate(SemanticErrorKind::EntropyMismatch(
                    mismatch_vars.join(", "),
                )));
            }

            if let Some(rule) = reconcile {
                for var in mismatch_vars {
                    if !rule.rules.contains_key(&var) {
                        return Err(analyzer
                            .annotate(SemanticErrorKind::EntropyMismatch(var)));
                    }
                }
            }

            analyzer
                .branch_contexts
                .insert(analyzer.current_branch.clone(), merged_state);
        }
        Statement::MatchEntropy {
            target,
            valid_branch,
            decayed_branch,
            pending_branch,
            consumed_branch,
        } => {
            let original_state = analyzer
                .branch_contexts
                .get(&analyzer.current_branch)
                .cloned()
                .unwrap_or_default();

            let mut context_candidates = Vec::new();

            if let Some((binding, branch_body)) = valid_branch {
                let saved_contexts = analyzer.branch_contexts.clone();
                let mut branch_contexts = analyzer.branch_contexts.clone();
                branch_contexts
                    .insert(analyzer.current_branch.clone(), original_state.clone());
                analyzer.branch_contexts = branch_contexts;
                analyzer
                    .branch_contexts
                    .get_mut(&analyzer.current_branch)
                    .unwrap()
                    .yields
                    .insert(binding.clone());

                let case_type = crate::analysis::expression::infer_expression_type(
                    analyzer, target,
                )?;
                analyzer.set_variable_type(binding, case_type);

                analyze_expression(analyzer, target)?;

                for stmt in branch_body {
                    analyze_statement(analyzer, stmt)?;
                }

                let end_state = analyzer
                    .branch_contexts
                    .get(&analyzer.current_branch)
                    .cloned()
                    .unwrap_or_default();
                context_candidates.push(end_state);
                analyzer.branch_contexts = saved_contexts;
            }

            if let Some((binding, branch_body)) = decayed_branch {
                let saved_contexts = analyzer.branch_contexts.clone();
                let mut branch_contexts = analyzer.branch_contexts.clone();
                branch_contexts
                    .insert(analyzer.current_branch.clone(), original_state.clone());
                analyzer.branch_contexts = branch_contexts;
                analyzer
                    .branch_contexts
                    .get_mut(&analyzer.current_branch)
                    .unwrap()
                    .yields
                    .insert(binding.clone());

                let case_type = crate::analysis::expression::infer_expression_type(
                    analyzer, target,
                )?;
                analyzer.set_variable_type(binding, case_type);

                analyze_expression(analyzer, target)?;

                for stmt in branch_body {
                    analyze_statement(analyzer, stmt)?;
                }

                let end_state = analyzer
                    .branch_contexts
                    .get(&analyzer.current_branch)
                    .cloned()
                    .unwrap_or_default();
                context_candidates.push(end_state);
                analyzer.branch_contexts = saved_contexts;
            }

            if let Some(branch_body) = pending_branch {
                let saved_contexts = analyzer.branch_contexts.clone();
                let mut branch_contexts = analyzer.branch_contexts.clone();
                branch_contexts
                    .insert(analyzer.current_branch.clone(), original_state.clone());
                analyzer.branch_contexts = branch_contexts;

                for stmt in branch_body {
                    analyze_statement(analyzer, stmt)?;
                }

                let end_state = analyzer
                    .branch_contexts
                    .get(&analyzer.current_branch)
                    .cloned()
                    .unwrap_or_default();
                context_candidates.push(end_state);
                analyzer.branch_contexts = saved_contexts;
            }

            if let Some(branch_body) = consumed_branch {
                let saved_contexts = analyzer.branch_contexts.clone();
                let mut branch_contexts = analyzer.branch_contexts.clone();
                branch_contexts
                    .insert(analyzer.current_branch.clone(), original_state.clone());
                analyzer.branch_contexts = branch_contexts;

                for stmt in branch_body {
                    analyze_statement(analyzer, stmt)?;
                }

                let end_state = analyzer
                    .branch_contexts
                    .get(&analyzer.current_branch)
                    .cloned()
                    .unwrap_or_default();
                context_candidates.push(end_state);
                analyzer.branch_contexts = saved_contexts;
            }

            let merged_state = context_candidates.into_iter().fold(
                original_state.clone(),
                |mut acc, s| {
                    acc.consumed.extend(s.consumed.into_iter());
                    acc.decayed.extend(s.decayed.into_iter());
                    acc.yields.extend(s.yields.into_iter());
                    acc
                },
            );

            analyzer
                .branch_contexts
                .insert(analyzer.current_branch.clone(), merged_state);
        }
        Statement::SpeculationMode(_) => {}
        Statement::Expression(expr) => {
            analyze_expression(analyzer, expr)?;
        }
        Statement::Print(expr) | Statement::Debug(expr) => {
            analyze_expression_nonconsuming(analyzer, expr)?;
            if !analyzer.capability_stack.is_empty()
                && !analyzer.is_capability_allowed("System.Log")
            {
                return Err(analyzer.annotate(
                    SemanticErrorKind::MissingCapability("System.Log".to_string()),
                ));
            }
        }
        Statement::Commit(body) => {
            for inner_stmt in body {
                analyze_statement(analyzer, inner_stmt)?;
            }
        }
        Statement::Yield(_) => {}
        Statement::For {
            item_name,
            mode,
            source,
            body,
            pacing_ms,
            max_ms,
        } => {
            let source_type = analyzer
                .get_variable_type(source)
                .unwrap_or(crate::analysis::types::Type::Unknown);

            let loop_item_type = match source_type {
                crate::analysis::types::Type::Struct(_)
                | crate::analysis::types::Type::Topology(_) => {
                    let mut item_fields = std::collections::HashMap::new();
                    item_fields.insert(
                        "key".to_string(),
                        crate::analysis::types::Type::String,
                    );
                    item_fields.insert(
                        "value".to_string(),
                        crate::analysis::types::Type::Unknown,
                    );
                    crate::analysis::types::Type::Struct(item_fields)
                }
                crate::analysis::types::Type::Array(inner) => *inner.clone(),
                other => other,
            };
            analyzer.set_variable_type(item_name, loop_item_type);

            if let ForMode::Consume = mode {
                analyzer.mark_consumed(source)?;
            }

            if let Some(max) = max_ms {
                if *max == 0 {
                    return Err(
                        analyzer.annotate(SemanticErrorKind::InvalidLoopBudget)
                    );
                }
            }

            for inner_stmt in body {
                analyze_statement(analyzer, inner_stmt)?;
            }

            if let Some(pacing) = pacing_ms {
                let body_cost = body.len() as u64;
                if body_cost > *pacing {
                    return Err(
                        analyzer.annotate(SemanticErrorKind::PacingViolation)
                    );
                }
            }
        }
        Statement::Speculate {
            max_ms: _,
            body,
            fallback,
        } => {
            let context_snapshot = analyzer.branch_contexts.clone();

            for stmt in body {
                analyze_statement(analyzer, stmt)?;
            }

            analyzer.branch_contexts = context_snapshot.clone();

            if let Some(fallback_body) = fallback {
                for stmt in fallback_body {
                    analyze_statement(analyzer, stmt)?;
                }
            }

            analyzer.branch_contexts = context_snapshot;
        }
        Statement::Collapse => {}
        Statement::SplitMap {
            item_name: _,
            mode: _,
            source,
            body,
            reconcile: _,
        } => {
            analyzer.mark_consumed(source)?;
            for inner_stmt in body {
                analyze_statement(analyzer, inner_stmt)?;
            }
        }
        Statement::Anchor(_)
        | Statement::Rewind(_)
        | Statement::Entangle { .. }
        | Statement::ChannelOpen { .. }
        | Statement::NetworkRequest { .. }
        | Statement::AcausalReset { .. } => {}
        Statement::Capability(cap) => {
            if !analyzer.is_capability_allowed(&cap.path) {
                return Err(analyzer.annotate(
                    SemanticErrorKind::MissingCapability(cap.path.clone()),
                ));
            }
        }
    }

    Ok(())
}

pub fn estimate_block_cost(
    analyzer: &EntropicAnalyzer,
    block: &[SpannedStatement],
) -> u64 {
    block
        .iter()
        .map(|stmt| estimate_statement_cost(analyzer, &stmt.stmt))
        .sum()
}

pub fn estimate_statement_cost(
    analyzer: &EntropicAnalyzer,
    stmt: &Statement,
) -> u64 {
    let base = 1;
    let extra = match stmt {
        Statement::NetworkRequest { .. } => 5,
        Statement::Split { .. }
        | Statement::Merge { .. }
        | Statement::Anchor(_)
        | Statement::Rewind(_)
        | Statement::Commit(_)
        | Statement::Send { .. }
        | Statement::ChannelOpen { .. }
        | Statement::ChannelSend { .. }
        | Statement::AcausalReset { .. }
        | Statement::Capability(_) => 0,
        Statement::Assignment { expr, .. } => {
            estimate_expression_cost(analyzer, expr)
        }
        Statement::FieldUpdate { value, .. } => {
            estimate_expression_cost(analyzer, value)
        }
        Statement::Expression(expr) => estimate_expression_cost(analyzer, expr),
        Statement::RelativisticBlock { body, .. } => {
            estimate_block_cost(analyzer, body)
        }
        Statement::Isolate(block) => estimate_block_cost(analyzer, &block.body),
        Statement::Inspect { body, .. } => estimate_block_cost(analyzer, body),
        Statement::Watchdog { recovery, .. } => {
            estimate_block_cost(analyzer, recovery)
        }
        Statement::If {
            then_branch,
            else_branch,
            ..
        } => {
            1 + estimate_block_cost(analyzer, then_branch).max(estimate_block_cost(
                analyzer,
                else_branch.as_ref().unwrap_or(&Vec::new()),
            ))
        }
        Statement::For { pacing_ms, .. } => pacing_ms.unwrap_or(1),
        Statement::Print(expr) | Statement::Debug(expr) => {
            1 + estimate_expression_cost(analyzer, expr)
        }
        Statement::Speculate { body, fallback, .. } => {
            let fallback_cost = estimate_block_cost(
                analyzer,
                fallback.as_ref().unwrap_or(&Vec::new()),
            );
            let body_cost = estimate_block_cost(analyzer, body);
            1 + body_cost + fallback_cost
        }
        Statement::Select { cases, timeout, .. } => {
            let case_max_cost = cases
                .iter()
                .map(|c| estimate_block_cost(analyzer, &c.body))
                .max()
                .unwrap_or(0);
            let timeout_cost = timeout
                .as_ref()
                .map(|b| estimate_block_cost(analyzer, b))
                .unwrap_or(0);
            1 + case_max_cost.max(timeout_cost)
        }
        Statement::MatchEntropy {
            valid_branch,
            decayed_branch,
            pending_branch,
            consumed_branch,
            ..
        } => {
            let valid_cost = valid_branch
                .as_ref()
                .map(|(_, body)| estimate_block_cost(analyzer, body))
                .unwrap_or(0);
            let decayed_cost = decayed_branch
                .as_ref()
                .map(|(_, body)| estimate_block_cost(analyzer, body))
                .unwrap_or(0);
            let pending_cost = pending_branch
                .as_ref()
                .map(|body| estimate_block_cost(analyzer, body))
                .unwrap_or(0);
            let consumed_cost = consumed_branch
                .as_ref()
                .map(|body| estimate_block_cost(analyzer, body))
                .unwrap_or(0);
            1 + valid_cost
                .max(decayed_cost)
                .max(pending_cost)
                .max(consumed_cost)
        }
        Statement::Collapse => 0,
        Statement::SplitMap { .. } => 1,
        Statement::Yield(_) => 0,
        Statement::Loop { max_ms, .. } => *max_ms,
        Statement::LoopTick { .. } => 1,
        Statement::Slice { .. } => 0,
        Statement::Await(_) => 1,
        Statement::SpeculationMode(_) => 0,
        Statement::Break => 0,
        Statement::Entangle { .. } => 0,
        Statement::TypeDecl { .. } => 0,
        Statement::RoutineDef { taking_ms, .. } => taking_ms.unwrap_or(0),
    };
    base + extra
}
