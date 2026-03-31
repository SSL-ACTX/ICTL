use crate::analysis_worker::AnalysisResults;
use dashmap::DashMap;
use ictl::analysis::statement::estimate_statement_cost;
use tower_lsp::lsp_types::*;

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

    let source = match results.source.as_ref() {
        Some(src) => src,
        None => return Ok(None),
    };

    if let Some(program) = &results.program {
        for timeline in &program.timelines {
            for stmt in &timeline.statements {
                // Check if position is within stmt.span
                if is_position_in_span(position, &stmt.span, source) {
                    let cost =
                        estimate_statement_cost(&results.analyzer, &stmt.stmt);
                    return Ok(Some(Hover {
                        contents: HoverContents::Scalar(MarkedString::String(
                            format!("WCET: {}ms", cost),
                        )),
                        range: None,
                    }));
                }
            }
        }
    }

    Ok(None)
}

fn is_position_in_span(
    pos: Position,
    span: &ictl::frontend::ast::Span,
    source: &str,
) -> bool {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analysis_worker::AnalysisResults;
    use dashmap::DashMap;
    use ictl::analysis::analyzer::EntropicAnalyzer;
    use ictl::frontend::ast::*;
    use tower_lsp::lsp_types::Url;

    #[tokio::test]
    async fn handle_hover_with_missing_source_does_not_panic() {
        let mut cache = DashMap::new();
        let program = Program {
            timelines: vec![TimelineBlock {
                time: TimeCoordinate::Global(0),
                statements: vec![SpannedStatement {
                    stmt: Statement::Expression(Expression::Identifier(
                        "x".to_string(),
                    )),
                    span: Span { start: 0, end: 1 },
                }],
            }],
        };

        cache.insert(
            Url::parse("file:///tmp/test.ictl").unwrap(),
            AnalysisResults {
                diagnostics: vec![],
                program: Some(program),
                analyzer: EntropicAnalyzer::new(),
                source: None,
            },
        );

        let result = handle_hover(
            HoverParams {
                text_document_position_params: TextDocumentPositionParams {
                    text_document: TextDocumentIdentifier {
                        uri: Url::parse("file:///tmp/test.ictl").unwrap(),
                    },
                    position: Position {
                        line: 0,
                        character: 0,
                    },
                },
                work_done_progress_params: Default::default(),
            },
            &cache,
        )
        .await
        .unwrap();

        assert_eq!(result, None);
    }
}
