use crate::frontend::ast::*;
use crate::frontend::parser::expressions::parse_expression;
use crate::frontend::parser::Rule;
use pest::iterators::Pair;
use std::collections::HashMap;

pub(crate) fn parse_statement(pair: Pair<Rule>) -> SpannedStatement {
    let span = Span {
        start: pair.as_span().start(),
        end: pair.as_span().end(),
    };

    let stmt = match pair.as_rule() {
        Rule::timeline_block => {
            let block = parse_timeline_block(pair);
            Statement::RelativisticBlock {
                time: block.time,
                body: block.statements,
            }
        }
        Rule::watchdog_stmt => {
            let mut inner = pair.into_inner();
            let target = inner
                .next()
                .map(|p| p.as_str().to_string())
                .unwrap_or_default();
            let timeout_ms = inner
                .next()
                .map(|p| p.as_str().parse::<u64>().unwrap_or(0))
                .unwrap_or(0);

            let mut recovery = Vec::new();
            if let Some(recovery_pair) = inner.next() {
                for stmt_pair in recovery_pair.into_inner() {
                    if let Some(actual_stmt) = stmt_pair.into_inner().next() {
                        recovery.push(parse_statement(actual_stmt));
                    }
                }
            }

            Statement::Watchdog {
                target,
                timeout_ms,
                recovery,
            }
        }
        Rule::isolate_stmt => {
            let mut inner = pair.into_inner();
            let mut name = None;
            let mut manifest = Manifest::default();
            let mut body = Vec::new();
            while let Some(current) = inner.next() {
                match current.as_rule() {
                    Rule::identifier => name = Some(current.as_str().to_string()),
                    Rule::manifest => manifest = parse_manifest(current),
                    Rule::statement => {
                        if let Some(s) = current.into_inner().next() {
                            body.push(parse_statement(s));
                        }
                    }
                    _ => {}
                }
            }
            Statement::Isolate(IsolateBlock {
                name,
                manifest,
                body,
            })
        }
        Rule::routine_stmt => {
            let mut inner = pair.into_inner();
            let name = inner
                .next()
                .map(|p| p.as_str().to_string())
                .unwrap_or_default();
            let mut params = Vec::new();
            let mut taking_ms: Option<u64> = None;
            let mut body = Vec::new();

            while let Some(current) = inner.next() {
                match current.as_rule() {
                    Rule::param_decl_list => {
                        for p in current.into_inner() {
                            let mut decl = p.into_inner();
                            if let (Some(mode), Some(param_name)) =
                                (decl.next(), decl.next())
                            {
                                let mode = match mode.as_str() {
                                    "consume" => ParamMode::Consume,
                                    "clone" => ParamMode::Clone,
                                    "decay" => ParamMode::Decay,
                                    _ => ParamMode::Peek,
                                };
                                params.push((mode, param_name.as_str().to_string()));
                            }
                        }
                    }
                    Rule::amount => {
                        taking_ms = current.as_str().parse::<u64>().ok();
                    }
                    Rule::statement => {
                        if let Some(s) = current.into_inner().next() {
                            body.push(parse_statement(s));
                        }
                    }
                    _ => {
                        if current.as_str() == "_" {
                            taking_ms = None;
                        }
                    }
                }
            }

            Statement::RoutineDef {
                name,
                params,
                taking_ms,
                body,
            }
        }
        Rule::require_decl => Statement::Capability(parse_capability(pair)),
        Rule::assignment_stmt => {
            let mut inner = pair.into_inner();
            let target = inner
                .next()
                .map(|p| p.as_str().to_string())
                .unwrap_or_default();
            let expr = inner
                .next()
                .map(parse_expression)
                .unwrap_or(Expression::Literal("void".into()));
            Statement::Assignment { target, expr }
        }
        Rule::open_chan_stmt => {
            let mut inner = pair.into_inner();
            let name = inner
                .next()
                .map(|p| p.as_str().to_string())
                .unwrap_or_default();
            let capacity = inner
                .next()
                .map(|p| p.as_str().parse::<usize>().unwrap_or(1))
                .unwrap_or(1);
            Statement::ChannelOpen { name, capacity }
        }
        Rule::chan_send_stmt => {
            let mut inner = pair.into_inner();
            let chan_id = inner
                .next()
                .map(|p| p.as_str().to_string())
                .unwrap_or_default();
            let value_id = inner
                .next()
                .map(|p| p.as_str().to_string())
                .unwrap_or_default();
            Statement::ChannelSend { chan_id, value_id }
        }
        Rule::split_stmt => {
            let mut inner = pair.into_inner();
            let parent = inner
                .next()
                .map(|p| p.as_str().to_string())
                .unwrap_or_default();
            let branches = inner
                .next()
                .map(|p| p.into_inner().map(|id| id.as_str().to_string()).collect())
                .unwrap_or_default();
            Statement::Split { parent, branches }
        }
        Rule::merge_stmt => {
            let mut inner = pair.into_inner();
            let branches = inner
                .next()
                .map(|p| p.into_inner().map(|id| id.as_str().to_string()).collect())
                .unwrap_or_default();
            let target = inner
                .next()
                .map(|p| p.as_str().to_string())
                .unwrap_or_default();
            let mut rules = HashMap::new();
            if let Some(res_rules) = inner.next() {
                for rule in res_rules.into_inner() {
                    let mut r_inner = rule.into_inner();
                    if let (Some(k), Some(v)) = (r_inner.next(), r_inner.next()) {
                        let value = v.as_str();
                        let strat = if value == "first_wins" {
                            ResolutionStrategy::FirstWins
                        } else if value == "decay" {
                            ResolutionStrategy::Decay
                        } else if let Some(inner) = value.strip_prefix("priority(") {
                            if let Some(branch_name) = inner.strip_suffix(")") {
                                ResolutionStrategy::Priority(branch_name.to_string())
                            } else {
                                ResolutionStrategy::Custom(value.to_string())
                            }
                        } else {
                            ResolutionStrategy::Priority(value.to_string())
                        };
                        rules.insert(k.as_str().to_string(), strat);
                    }
                }
            }
            Statement::Merge {
                branches,
                target,
                resolutions: MergeResolution { rules, auto: false },
            }
        }
        Rule::commit_stmt => {
            let mut body = Vec::new();
            for stmt_pair in pair.into_inner() {
                if let Some(actual_stmt) = stmt_pair.into_inner().next() {
                    body.push(parse_statement(actual_stmt));
                }
            }
            Statement::Commit(body)
        }
        Rule::speculate_stmt => {
            let mut inner = pair.into_inner();
            let max_ms = inner
                .next()
                .and_then(|p| p.as_str().parse::<u64>().ok())
                .unwrap_or(0);
            let mut body = Vec::new();
            let mut fallback = None;

            for element in inner {
                match element.as_rule() {
                    Rule::statement => {
                        if let Some(actual_stmt) = element.into_inner().next() {
                            body.push(parse_statement(actual_stmt));
                        }
                    }
                    Rule::fallback_stmt => {
                        let mut fb = Vec::new();
                        for stmt_pair in element.into_inner() {
                            if let Some(actual_stmt) = stmt_pair.into_inner().next()
                            {
                                fb.push(parse_statement(actual_stmt));
                            }
                        }
                        fallback = Some(fb);
                    }
                    _ => {}
                }
            }

            Statement::Speculate {
                max_ms,
                body,
                fallback,
            }
        }
        Rule::collapse_stmt => Statement::Collapse,
        Rule::speculation_mode_stmt => {
            let mode_str = pair
                .into_inner()
                .next()
                .map(|p| p.as_str())
                .unwrap_or("selective");
            let mode = match mode_str {
                "full" => SpeculationCommitMode::Full,
                _ => SpeculationCommitMode::Selective,
            };
            Statement::SpeculationMode(mode)
        }
        Rule::select_stmt => {
            let mut inner = pair.into_inner();
            let max_ms = inner
                .next()
                .and_then(|p| p.as_str().parse::<u64>().ok())
                .unwrap_or(0);
            let mut cases = Vec::new();
            let mut timeout = None;
            let mut reconcile = None;

            for element in inner {
                match element.as_rule() {
                    Rule::select_case => {
                        let mut case_inner = element.into_inner();
                        let binding = case_inner
                            .next()
                            .map(|p| p.as_str().to_string())
                            .unwrap_or_default();
                        let source = case_inner
                            .next()
                            .map(parse_expression)
                            .unwrap_or(Expression::Literal("".into()));
                        let body = case_inner
                            .next()
                            .map(|stmt_block| {
                                stmt_block
                                    .into_inner()
                                    .filter_map(|s| s.into_inner().next())
                                    .map(parse_statement)
                                    .collect()
                            })
                            .unwrap_or_default();
                        cases.push(SelectCase {
                            binding,
                            source,
                            body,
                        });
                    }
                    Rule::timeout_clause => {
                        if let Some(block) = element.into_inner().next() {
                            let body = block
                                .into_inner()
                                .filter_map(|s| s.into_inner().next())
                                .map(parse_statement)
                                .collect();
                            timeout = Some(body);
                        }
                    }
                    Rule::resolution_rules => {
                        let mut rules = HashMap::new();
                        for rule in element.into_inner() {
                            let mut r_inner = rule.into_inner();
                            if let (Some(k), Some(v)) =
                                (r_inner.next(), r_inner.next())
                            {
                                let value = v.as_str();
                                let strat = if value == "first_wins" {
                                    ResolutionStrategy::FirstWins
                                } else if value == "decay" {
                                    ResolutionStrategy::Decay
                                } else if let Some(inner) =
                                    value.strip_prefix("priority(")
                                {
                                    if let Some(branch_name) =
                                        inner.strip_suffix(")")
                                    {
                                        ResolutionStrategy::Priority(
                                            branch_name.to_string(),
                                        )
                                    } else {
                                        ResolutionStrategy::Custom(value.to_string())
                                    }
                                } else {
                                    ResolutionStrategy::Priority(value.to_string())
                                };
                                rules.insert(k.as_str().to_string(), strat);
                            }
                        }
                        reconcile = Some(MergeResolution { rules, auto: false });
                    }
                    Rule::reconcile_clause => {
                        if element.as_str().contains("auto") {
                            reconcile = Some(MergeResolution {
                                rules: HashMap::new(),
                                auto: true,
                            });
                        }
                    }
                    _ => {}
                }
            }
            Statement::Select {
                max_ms,
                cases,
                timeout,
                reconcile,
            }
        }
        Rule::match_entropy_stmt => {
            let mut inner = pair.into_inner();
            let target = inner
                .next()
                .map(|p| p.as_str().to_string())
                .unwrap_or_default();
            let mut valid_branch = None;
            let mut decayed_branch = None;
            let mut pending_branch = None;
            let mut consumed_branch = None;

            for element in inner {
                if element.as_rule() == Rule::entropy_branch {
                    let text = element.as_str().trim();
                    let is_valid = text.starts_with("Valid(");
                    let is_decayed = text.starts_with("Decayed(");
                    let is_pending = text.starts_with("Pending(");
                    let is_consumed = text.starts_with("Consumed");

                    let mut branch_inner = element.into_inner();
                    if is_valid || is_decayed || is_pending {
                        let var_name = branch_inner
                            .next()
                            .map(|p| p.as_str().to_string())
                            .unwrap_or_default();
                        let body = branch_inner
                            .next()
                            .map(|stmt_block| {
                                stmt_block
                                    .into_inner()
                                    .filter_map(|s| s.into_inner().next())
                                    .map(parse_statement)
                                    .collect()
                            })
                            .unwrap_or_default();
                        if is_valid {
                            valid_branch = Some((var_name, body));
                        } else if is_decayed {
                            decayed_branch = Some((var_name, body));
                        } else if is_pending {
                            pending_branch = Some(body);
                        }
                    } else if is_consumed {
                        let body = branch_inner
                            .next()
                            .map(|stmt_block| {
                                stmt_block
                                    .into_inner()
                                    .filter_map(|s| s.into_inner().next())
                                    .map(parse_statement)
                                    .collect()
                            })
                            .unwrap_or_default();
                        consumed_branch = Some(body);
                    }
                }
            }
            Statement::MatchEntropy {
                target,
                valid_branch,
                decayed_branch,
                pending_branch,
                consumed_branch,
            }
        }
        Rule::anchor_stmt => Statement::Anchor(
            pair.into_inner()
                .next()
                .map(|p| p.as_str().to_string())
                .unwrap_or_default(),
        ),
        Rule::rewind_stmt => Statement::Rewind(
            pair.into_inner()
                .next()
                .map(|p| p.as_str().to_string())
                .unwrap_or_default(),
        ),
        Rule::network_request_stmt => {
            let domain = pair
                .into_inner()
                .next()
                .map(|p| p.as_str().replace("\"", ""))
                .unwrap_or_default();
            Statement::NetworkRequest { domain }
        }
        Rule::await_stmt => {
            let target = pair
                .into_inner()
                .next()
                .map(|p| p.as_str().to_string())
                .unwrap_or_default();
            Statement::Await(target)
        }
        Rule::print_stmt => {
            let mut inner = pair.into_inner();
            let expr = inner
                .next()
                .map(parse_expression)
                .unwrap_or(Expression::Literal("".into()));
            Statement::Print(expr)
        }
        Rule::debug_stmt => {
            let mut inner = pair.into_inner();
            let expr = inner
                .next()
                .map(parse_expression)
                .unwrap_or(Expression::Literal("".into()));
            Statement::Debug(expr)
        }
        Rule::break_stmt => Statement::Break,
        Rule::if_stmt => {
            let mut inner = pair.into_inner();
            let condition = inner
                .next()
                .map(parse_expression)
                .unwrap_or(Expression::Literal("false".into()));
            let then_branch = if let Some(b) = inner.next() {
                b.into_inner()
                    .filter_map(|stmt_pair| stmt_pair.into_inner().next())
                    .map(parse_statement)
                    .collect()
            } else {
                Vec::new()
            };
            let else_branch = if let Some(else_pair) = inner.next() {
                Some(
                    else_pair
                        .into_inner()
                        .filter_map(|stmt_pair| stmt_pair.into_inner().next())
                        .map(parse_statement)
                        .collect(),
                )
            } else {
                None
            };
            let reconcile_rules = if let Some(reconcile_pair) = inner.next() {
                let mut rules = HashMap::new();
                let mut is_auto = false;
                for child in reconcile_pair.into_inner() {
                    if child.as_rule() == Rule::resolution_rules {
                        for rule in child.into_inner() {
                            let mut r_inner = rule.into_inner();
                            if let (Some(k), Some(v)) =
                                (r_inner.next(), r_inner.next())
                            {
                                let value = v.as_str();
                                let strat = if value == "first_wins" {
                                    ResolutionStrategy::FirstWins
                                } else if value == "decay" {
                                    ResolutionStrategy::Decay
                                } else if let Some(inner) =
                                    value.strip_prefix("priority(")
                                {
                                    if let Some(branch_name) =
                                        inner.strip_suffix(")")
                                    {
                                        ResolutionStrategy::Priority(
                                            branch_name.to_string(),
                                        )
                                    } else {
                                        ResolutionStrategy::Custom(value.to_string())
                                    }
                                } else {
                                    ResolutionStrategy::Priority(value.to_string())
                                };
                                rules.insert(k.as_str().to_string(), strat);
                            }
                        }
                    } else if child.as_str() == "auto" {
                        is_auto = true;
                    }
                }
                Some(MergeResolution {
                    rules,
                    auto: is_auto,
                })
            } else {
                None
            };

            Statement::If {
                condition,
                then_branch,
                else_branch,
                reconcile: reconcile_rules,
            }
        }
        Rule::inspect_stmt => {
            let mut inner = pair.into_inner();
            let target = inner
                .next()
                .map(|p| p.as_str().to_string())
                .unwrap_or_default();
            let mut body = Vec::new();
            for stmt in inner {
                if let Some(actual_stmt) = stmt.into_inner().next() {
                    body.push(parse_statement(actual_stmt));
                }
            }
            Statement::Inspect { target, body }
        }
        Rule::for_stmt => {
            let mut inner = pair.into_inner();
            let item = inner
                .next()
                .map(|p| p.as_str().to_string())
                .unwrap_or_default();
            let mode = inner
                .next()
                .map(|p| match p.as_str() {
                    "consume" => ForMode::Consume,
                    _ => ForMode::Clone,
                })
                .unwrap_or(ForMode::Consume);
            let source = inner
                .next()
                .map(|p| p.as_str().to_string())
                .unwrap_or_default();

            let mut pacing_ms = None;
            let mut max_ms = None;
            let mut body = Vec::new();

            for next in inner {
                match next.as_rule() {
                    Rule::pacing_opt => {
                        let amount = next
                            .into_inner()
                            .next()
                            .and_then(|p| p.as_str().parse::<u64>().ok());
                        pacing_ms = amount;
                    }
                    Rule::max_opt => {
                        let amount = next
                            .into_inner()
                            .next_back()
                            .and_then(|p| p.as_str().parse::<u64>().ok());
                        max_ms = amount;
                    }
                    Rule::statement => {
                        if let Some(actual_stmt) = next.into_inner().next() {
                            body.push(parse_statement(actual_stmt));
                        }
                    }
                    _ => {}
                }
            }

            Statement::For {
                item_name: item,
                mode,
                source,
                body,
                pacing_ms,
                max_ms,
            }
        }
        Rule::split_map_stmt => {
            let mut inner = pair.into_inner();
            let item_name = inner
                .next()
                .map(|p| p.as_str().to_string())
                .unwrap_or_default();
            let mode = inner.next().map(|p| p.as_str()).unwrap_or("consume");
            let source = inner
                .next()
                .map(|p| p.as_str().to_string())
                .unwrap_or_default();
            let mode_enum = if mode == "clone" {
                ForMode::Clone
            } else {
                ForMode::Consume
            };

            let mut body = Vec::new();
            let mut reconcile = None;

            for next in inner {
                match next.as_rule() {
                    Rule::statement => {
                        if let Some(actual_stmt) = next.into_inner().next() {
                            body.push(parse_statement(actual_stmt));
                        }
                    }
                    Rule::resolution_rules => {
                        let mut rules = HashMap::new();
                        for rule in next.into_inner() {
                            let mut r_inner = rule.into_inner();
                            if let (Some(k), Some(v)) =
                                (r_inner.next(), r_inner.next())
                            {
                                let strat = match v.as_str() {
                                    "first_wins" => ResolutionStrategy::FirstWins,
                                    "decay" => ResolutionStrategy::Decay,
                                    p if p.starts_with("priority(") => {
                                        let name = p
                                            .trim_start_matches("priority(")
                                            .trim_end_matches(")");
                                        ResolutionStrategy::Priority(
                                            name.to_string(),
                                        )
                                    }
                                    _ => ResolutionStrategy::Custom(
                                        v.as_str().to_string(),
                                    ),
                                };
                                rules.insert(k.as_str().to_string(), strat);
                            }
                        }
                        reconcile = Some(MergeResolution { rules, auto: false });
                    }
                    _ => {}
                }
            }

            Statement::SplitMap {
                item_name,
                mode: mode_enum,
                source,
                body,
                reconcile,
            }
        }
        Rule::loop_stmt => {
            let mut inner = pair.into_inner();
            let max_value = inner
                .next()
                .and_then(|p| p.as_str().parse::<u64>().ok())
                .unwrap_or(0);
            let mut body = Vec::new();
            for stmt_pair in inner {
                if stmt_pair.as_rule() == Rule::statement {
                    if let Some(actual_stmt) = stmt_pair.into_inner().next() {
                        body.push(parse_statement(actual_stmt));
                    }
                }
            }
            Statement::Loop {
                max_ms: max_value,
                body,
            }
        }
        Rule::yield_stmt => {
            let item = pair
                .into_inner()
                .next()
                .map(|p| p.as_str().to_string())
                .unwrap_or_default();
            Statement::Yield(item)
        }
        Rule::reset_stmt => {
            let mut inner = pair.into_inner();
            let target = inner
                .next()
                .map(|p| p.as_str().to_string())
                .unwrap_or_default();
            let anchor_name = inner
                .next()
                .map(|p| p.as_str().to_string())
                .unwrap_or_default();
            Statement::AcausalReset {
                target,
                anchor_name,
            }
        }
        _ => Statement::Expression(parse_expression(pair)),
    };

    SpannedStatement { stmt, span }
}

pub(crate) fn parse_timeline_block(pair: Pair<Rule>) -> TimelineBlock {
    let mut inner = pair.into_inner();
    let time_coord_pair = inner.next().expect("Timeline missing time");
    let time_pair = time_coord_pair
        .into_inner()
        .next()
        .expect("Invalid time structure");

    let time = match time_pair.as_rule() {
        Rule::absolute_time => TimeCoordinate::Global(
            time_pair.as_str().replace("ms", "").parse().unwrap_or(0),
        ),
        Rule::relative_time => TimeCoordinate::Relative(
            time_pair
                .as_str()
                .replace("+", "")
                .replace("ms", "")
                .parse()
                .unwrap_or(0),
        ),
        Rule::branch_name => TimeCoordinate::Branch(time_pair.as_str().to_string()),
        _ => TimeCoordinate::Global(0),
    };

    let mut statements = Vec::new();
    if let Some(block_inner) = inner.next() {
        for stmt_pair in block_inner.into_inner() {
            if let Some(actual_stmt) = stmt_pair.into_inner().next() {
                let spanned = parse_statement(actual_stmt);
                statements.push(spanned);
            }
        }
    }
    TimelineBlock { time, statements }
}

fn parse_manifest(pair: Pair<Rule>) -> Manifest {
    let mut manifest = Manifest::default();
    for item in pair.into_inner() {
        match item.as_rule() {
            Rule::resource_decl => {
                let mut inner = item.into_inner();
                let res_type = inner.next().map(|p| p.as_str()).unwrap_or("");
                let amount = inner
                    .next()
                    .map(|p| p.as_str().parse::<u64>().unwrap_or(0))
                    .unwrap_or(0);
                match res_type {
                    "cpu" => manifest.cpu_budget_ms = Some(amount),
                    "memory" => manifest.memory_budget_bytes = Some(amount),
                    _ => {}
                }
            }
            Rule::require_decl => manifest.capabilities.push(parse_capability(item)),
            _ => {}
        }
    }
    manifest
}

fn parse_capability(pair: Pair<Rule>) -> Capability {
    let mut inner = pair.into_inner();
    let path = inner
        .next()
        .map(|p| p.as_str().to_string())
        .unwrap_or_default();
    let mut parameters = HashMap::new();
    if let Some(params_pair) = inner.next() {
        for p in params_pair.into_inner() {
            let mut p_inner = p.into_inner();
            if let (Some(k), Some(v)) = (p_inner.next(), p_inner.next()) {
                parameters
                    .insert(k.as_str().to_string(), v.as_str().replace("\"", ""));
            }
        }
    }
    Capability { path, parameters }
}
