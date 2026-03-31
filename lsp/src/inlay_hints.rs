use tower_lsp::lsp_types::*;
use crate::analysis_worker::AnalysisResults;
use dashmap::DashMap;
use ictl::analysis::statement::estimate_block_cost;
use ictl::frontend::ast::*;

pub async fn handle_inlay_hints(
    params: InlayHintParams,
    cache: &DashMap<Url, AnalysisResults>,
) -> tower_lsp::jsonrpc::Result<Option<Vec<InlayHint>>> {
    let uri = params.text_document.uri;
    let results = match cache.get(&uri) {
        Some(r) => r,
        None => return Ok(None),
    };

    let mut hints = Vec::new();

    if let Some(program) = &results.program {
        for timeline in &program.timelines {
            for stmt in &timeline.statements {
                walk_statement(stmt, &results, &mut hints);
            }
        }
    }

    Ok(Some(hints))
}

fn walk_statement(
    stmt: &SpannedStatement,
    results: &AnalysisResults,
    hints: &mut Vec<InlayHint>,
) {
    let source = results.analyzer.source.as_ref().unwrap();
    
    match &stmt.stmt {
        Statement::If { then_branch, else_branch, .. } => {
            let then_cost = estimate_block_cost(&results.analyzer, then_branch);
            let else_cost = else_branch.as_ref().map(|b| estimate_block_cost(&results.analyzer, b)).unwrap_or(0);
            let max = then_cost.max(else_cost);

            if then_cost < max {
                add_padding_hint(then_branch, max - then_cost, hints, source);
            }
            if else_cost < max && else_branch.is_some() {
                add_padding_hint(else_branch.as_ref().unwrap(), max - else_cost, hints, source);
            }
        }
        Statement::Loop { max_ms, body } => {
            let cost = estimate_block_cost(&results.analyzer, body);
            if cost < *max_ms {
                add_padding_hint(body, *max_ms - cost, hints, source);
            }
        }
        Statement::Speculate { max_ms, body, .. } => {
            let cost = estimate_block_cost(&results.analyzer, body);
            if cost < *max_ms {
                add_padding_hint(body, *max_ms - cost, hints, source);
            }
        }
        _ => {}
    }
}

fn add_padding_hint(block: &[SpannedStatement], padding: u64, hints: &mut Vec<InlayHint>, source: &str) {
    if let Some(last) = block.last() {
        let abs_pos = last.span.end;
        let line_text = &source[..abs_pos];
        let line = line_text.lines().count() as u32 - 1;
        let col = line_text.lines().last().map(|l| l.len()).unwrap_or(0) as u32;

        hints.push(InlayHint {
            position: Position { line, character: col },
            label: InlayHintLabel::String(format!(" // ⏳ VM pads +{}ms", padding)),
            kind: Some(InlayHintKind::PARAMETER),
            padding_left: Some(true),
            padding_right: Some(false),
            data: None,
            tooltip: None,
            text_edits: None,
        });
    }
}
