use tower_lsp::lsp_types::*;
use crate::analysis_worker::AnalysisResults;
use ictl::frontend::ast::*;
use dashmap::DashMap;

pub async fn handle_tokens(
    params: SemanticTokensParams,
    cache: &DashMap<Url, AnalysisResults>,
) -> tower_lsp::jsonrpc::Result<Option<SemanticTokensResult>> {
    let uri = params.text_document.uri;
    let results = match cache.get(&uri) {
        Some(r) => r,
        None => return Ok(None),
    };

    let mut tokens = Vec::new();
    let mut last_line = 0;
    let mut last_start = 0;

    if let Some(program) = &results.program {
        for timeline in &program.timelines {
            for stmt in &timeline.statements {
                let state = results.analyzer.span_states.get(&stmt.span);
                
                // For each statement, we walk its expressions to find identifiers
                // This is a simplified visitor
                walk_statement(stmt, state, &mut tokens, &mut last_line, &mut last_start, &results.analyzer.source.as_ref().unwrap());
            }
        }
    }

    Ok(Some(SemanticTokensResult::Tokens(SemanticTokens {
        result_id: None,
        data: tokens,
    })))
}

fn walk_statement(
    stmt: &SpannedStatement,
    state: Option<&ictl::analysis::analyzer::BranchState>,
    tokens: &mut Vec<SemanticToken>,
    last_line: &mut u32,
    last_start: &mut u32,
    source: &str,
) {
    match &stmt.stmt {
        Statement::Assignment { target, expr, .. } => {
            push_variable_token(target, &stmt.span, state, tokens, last_line, last_start, source);
            walk_expression(expr, &stmt.span, state, tokens, last_line, last_start, source);
        }
        Statement::Expression(expr) => {
            walk_expression(expr, &stmt.span, state, tokens, last_line, last_start, source);
        }
        Statement::If { condition, .. } => {
            walk_expression(condition, &stmt.span, state, tokens, last_line, last_start, source);
            // Internal branches are handled in the main loop
        }
        // ... add more as needed
        _ => {}
    }
}

fn walk_expression(
    expr: &Expression,
    span: &Span,
    state: Option<&ictl::analysis::analyzer::BranchState>,
    tokens: &mut Vec<SemanticToken>,
    last_line: &mut u32,
    last_start: &mut u32,
    source: &str,
) {
    match expr {
        Expression::Identifier(name) => {
            push_variable_token(name, span, state, tokens, last_line, last_start, source);
        }
        Expression::BinaryOp { left, right, .. } => {
            walk_expression(left, span, state, tokens, last_line, last_start, source);
            walk_expression(right, span, state, tokens, last_line, last_start, source);
        }
        Expression::Call { args, .. } => {
            for arg in args {
                walk_expression(arg, span, state, tokens, last_line, last_start, source);
            }
        }
        Expression::FieldAccess { target, .. } => {
            walk_expression(target, span, state, tokens, last_line, last_start, source);
        }
        Expression::StructLit(fields) | Expression::TopologyLit(fields) => {
            for v in fields.values() {
                walk_expression(v, span, state, tokens, last_line, last_start, source);
            }
        }
        _ => {}
    }
}

fn push_variable_token(
    name: &str,
    span: &Span,
    state: Option<&ictl::analysis::analyzer::BranchState>,
    tokens: &mut Vec<SemanticToken>,
    last_line: &mut u32,
    last_start: &mut u32,
    source: &str,
) {
    // Approximate position by searching name in span text
    let text = &source[span.start..span.end];
    if let Some(offset) = text.find(name) {
        let abs_pos = span.start + offset;
        let line_text = &source[..abs_pos];
        let line = line_text.lines().count() as u32 - 1;
        let col = line_text.lines().last().map(|l| l.len()).unwrap_or(0) as u32;

        let delta_line = line - *last_line;
        let delta_start = if delta_line == 0 {
            col - *last_start
        } else {
            col
        };

        let token_type = if let Some(s) = state {
            if s.consumed.contains(name) {
                1 // COMMENT (for gray/strikethrough)
            } else if s.decayed.contains(name) {
                0 // VARIABLE (yellow/warning)
            } else {
                0 // VARIABLE (bright)
            }
        } else {
            0
        };

        tokens.push(SemanticToken {
            delta_line,
            delta_start,
            length: name.len() as u32,
            token_type,
            token_modifiers_bitset: 0,
        });

        *last_line = line;
        *last_start = col;
    }
}
