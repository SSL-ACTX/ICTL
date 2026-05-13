use ictl_core::Program;
use pest::Parser;
use pest_derive::Parser;

pub mod expressions;
pub mod statements;

#[derive(Parser)]
#[grammar = "ictl.pest"]
pub struct IctlParser;

pub fn parse_ictl(source: &str) -> anyhow::Result<Program> {
    let mut pairs = IctlParser::parse(Rule::program, source)?;
    let mut timelines = Vec::new();

    if let Some(program_pair) = pairs.next() {
        for pair in program_pair.into_inner() {
            if pair.as_rule() == Rule::timeline_block {
                timelines.push(statements::parse_timeline_block(pair));
            }
        }
    }

    Ok(Program { timelines })
}
