use tower_lsp::lsp_types::*;
use crate::analysis_worker::AnalysisResults;
use dashmap::DashMap;
use ictl::analysis::statement::estimate_statement_cost;

pub async fn handle_hover(
    params: HoverParams,
    cache: &DashMap<Url, AnalysisResults>,
) -> tower_lsp::jsonrpc::Result<Option<Hover>> {
    let uri = params.text_document_position_params.text_document.uri;
    let position = params.text_document_position_params.position;
    
    let results = match cache.get(&uri) {
        Some(r) => r,
        None => return Ok(None),
    };

    if let Some(program) = &results.program {
        for timeline in &program.timelines {
            for stmt in &timeline.statements {
                // Check if position is within stmt.span
                if is_position_in_span(position, &stmt.span, &results.analyzer.source.as_ref().unwrap()) {
                    let cost = estimate_statement_cost(&results.analyzer, &stmt.stmt);
                    return Ok(Some(Hover {
                        contents: HoverContents::Scalar(MarkedString::String(format!("WCET: {}ms", cost))),
                        range: None,
                    }));
                }
            }
        }
    }

    Ok(None)
}

fn is_position_in_span(pos: Position, span: &ictl::frontend::ast::Span, source: &str) -> bool {
    let mut offset = 0;
    for (i, line) in source.lines().enumerate() {
        if i as u32 == pos.line {
            let start_offset = offset + pos.character as usize;
            return start_offset >= span.start && start_offset <= span.end;
        }
        offset += line.len() + 1; // +1 for newline
    }
    false
}
