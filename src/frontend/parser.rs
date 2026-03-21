// src/frontend/parser.rs
use super::ast::*;
use pest::Parser;
use pest_derive::Parser;
use std::collections::HashMap;

#[derive(Parser)]
#[grammar = "frontend/ictl.pest"]
pub struct IctlParser;

pub fn parse_ictl(input: &str) -> anyhow::Result<Program> {
    let pairs = IctlParser::parse(Rule::program, input)?;
    let mut timelines = Vec::new();
    for pair in pairs {
        if let Rule::program = pair.as_rule() {
            for inner in pair.into_inner() {
                if inner.as_rule() == Rule::timeline_block {
                    timelines.push(parse_timeline_block(inner));
                }
            }
        }
    }
    Ok(Program { timelines })
}

fn parse_timeline_block(pair: pest::iterators::Pair<Rule>) -> TimelineBlock {
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

fn parse_statement(
    pair: pest::iterators::Pair<Rule>,
) -> crate::frontend::ast::SpannedStatement {
    let span = crate::frontend::ast::Span {
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
                // If the 'recovery' keyword was present, the next token is the statement_block
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
                            // direct branch name as priority for merge compatibility
                            ResolutionStrategy::Priority(value.to_string())
                        };
                        rules.insert(k.as_str().to_string(), strat);
                    }
                }
            }
            Statement::Merge {
                branches,
                target,
                resolutions: MergeResolution { rules },
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
                "full" => crate::frontend::ast::SpeculationCommitMode::Full,
                _ => crate::frontend::ast::SpeculationCommitMode::Selective,
            };
            Statement::SpeculationMode(mode)
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
        Rule::break_stmt => Statement::Break,
        Rule::if_stmt => {
            let mut inner = pair.into_inner();
            let condition = inner
                .next()
                .map(parse_expression)
                .unwrap_or(Expression::Literal("false".into()));
            let then_block_pair = inner.next();
            let then_branch = if let Some(b) = then_block_pair {
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
                for rule in reconcile_pair.into_inner() {
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
                            // direct branch name as priority for merge compatibility
                            ResolutionStrategy::Priority(value.to_string())
                        };
                        rules.insert(k.as_str().to_string(), strat);
                    }
                }
                Some(MergeResolution { rules })
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
        Rule::for_stmt => {
            let mut inner = pair.into_inner();
            let item = inner
                .next()
                .map(|p| p.as_str().to_string())
                .unwrap_or_default();
            let mode = inner
                .next()
                .map(|p| match p.as_str() {
                    "consume" => crate::frontend::ast::ForMode::Consume,
                    _ => crate::frontend::ast::ForMode::Clone,
                })
                .unwrap_or(crate::frontend::ast::ForMode::Consume);
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
                crate::frontend::ast::ForMode::Clone
            } else {
                crate::frontend::ast::ForMode::Consume
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
                                    "first_wins" => crate::frontend::ast::ResolutionStrategy::FirstWins,
                                    "decay" => crate::frontend::ast::ResolutionStrategy::Decay,
                                    p if p.starts_with("priority(") => {
                                        let name = p.trim_start_matches("priority(").trim_end_matches(")");
                                        crate::frontend::ast::ResolutionStrategy::Priority(name.to_string())
                                    }
                                    _ => crate::frontend::ast::ResolutionStrategy::Custom(v.as_str().to_string()),
                                };
                                rules.insert(k.as_str().to_string(), strat);
                            }
                        }
                        reconcile =
                            Some(crate::frontend::ast::MergeResolution { rules });
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

    crate::frontend::ast::SpannedStatement { stmt, span }
}

fn parse_manifest(pair: pest::iterators::Pair<Rule>) -> Manifest {
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

fn parse_capability(pair: pest::iterators::Pair<Rule>) -> Capability {
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

fn parse_expression(pair: pest::iterators::Pair<Rule>) -> Expression {
    match pair.as_rule() {
        Rule::expression
        | Rule::relational_expr
        | Rule::additive_expr
        | Rule::multiplicative_expr => {
            let mut inner = pair.into_inner();
            let first = inner.next().map(parse_expression);
            if first.is_none() {
                return Expression::Literal("void".into());
            }
            let mut left = first.unwrap();
            while let Some(op_pair) = inner.next() {
                let op = match op_pair.as_str() {
                    "+" => crate::frontend::ast::BinaryOperator::Add,
                    "-" => crate::frontend::ast::BinaryOperator::Sub,
                    "*" => crate::frontend::ast::BinaryOperator::Mul,
                    "/" => crate::frontend::ast::BinaryOperator::Div,
                    "==" => crate::frontend::ast::BinaryOperator::Eq,
                    "!=" => crate::frontend::ast::BinaryOperator::Neq,
                    "<" => crate::frontend::ast::BinaryOperator::Lt,
                    ">" => crate::frontend::ast::BinaryOperator::Gt,
                    "<=" => crate::frontend::ast::BinaryOperator::Le,
                    ">=" => crate::frontend::ast::BinaryOperator::Ge,
                    _ => crate::frontend::ast::BinaryOperator::Eq,
                };
                if let Some(right) = inner.next() {
                    let right_expr = parse_expression(right);
                    left = Expression::BinaryOp {
                        left: Box::new(left),
                        op,
                        right: Box::new(right_expr),
                    };
                }
            }
            left
        }
        Rule::unary_expr => {
            let mut inner = pair.into_inner();
            if let Some(first) = inner.next() {
                if first.as_str() == "-" {
                    let expr = parse_expression(inner.next().unwrap());
                    let zero = Expression::Integer(0);
                    return Expression::BinaryOp {
                        left: Box::new(zero),
                        op: crate::frontend::ast::BinaryOperator::Sub,
                        right: Box::new(expr),
                    };
                }
                parse_expression(first)
            } else {
                Expression::Literal("void".into())
            }
        }
        Rule::primary_expr => {
            let inner = pair.into_inner().next().unwrap();
            parse_expression(inner)
        }
        Rule::integer_literal => {
            Expression::Integer(pair.as_str().parse::<i64>().unwrap_or(0))
        }
        Rule::string_literal => Expression::Literal(pair.as_str().replace("\"", "")),
        Rule::identifier_expr | Rule::identifier => {
            Expression::Identifier(pair.as_str().to_string())
        }
        Rule::clone_op => Expression::CloneOp(
            pair.into_inner()
                .next()
                .map(|p| p.as_str().to_string())
                .unwrap_or_default(),
        ),
        Rule::chan_recv_expr => Expression::ChannelReceive(
            pair.into_inner()
                .next()
                .map(|p| p.as_str().to_string())
                .unwrap_or_default(),
        ),
        Rule::struct_lit => {
            let mut fields = HashMap::new();
            if let Some(params) = pair.into_inner().next() {
                for p in params.into_inner() {
                    let mut p_inner = p.into_inner();
                    if let (Some(k), Some(v)) = (p_inner.next(), p_inner.next()) {
                        fields.insert(k.as_str().to_string(), parse_expression(v));
                    }
                }
            }
            Expression::StructLit(fields)
        }
        Rule::array_lit => {
            let mut elements = Vec::new();
            for expr_pair in pair.into_inner() {
                elements.push(parse_expression(expr_pair));
            }
            Expression::ArrayLiteral(elements)
        }
        Rule::field_access => {
            let mut inner = pair.into_inner();
            let parent = inner
                .next()
                .map(|p| p.as_str().to_string())
                .unwrap_or_default();
            let field = inner
                .next()
                .map(|p| p.as_str().to_string())
                .unwrap_or_default();
            Expression::FieldAccess { parent, field }
        }
        _ => Expression::Literal(pair.as_str().to_string()),
    }
}
