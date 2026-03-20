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
                        let strat = match v.as_str() {
                            "first_wins" => ResolutionStrategy::FirstWins,
                            p => ResolutionStrategy::Priority(p.to_string()),
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
        Rule::expression => {
            if let Some(i) = pair.into_inner().next() {
                parse_expression(i)
            } else {
                Expression::Literal("void".into())
            }
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
