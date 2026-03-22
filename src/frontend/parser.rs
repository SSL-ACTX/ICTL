use crate::frontend::ast::*;
use pest::Parser;
use pest_derive::Parser;

mod expressions;
mod statements;

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
                    timelines.push(statements::parse_timeline_block(inner));
                }
            }
        }
    }
    Ok(Program { timelines })
}
