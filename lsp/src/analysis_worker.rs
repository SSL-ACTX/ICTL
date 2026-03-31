use ictl::analysis::analyzer::{BranchState, EntropicAnalyzer};
use ictl::frontend::ast::Program;
use ictl::frontend::parser::parse_ictl;
use std::collections::HashMap;
use tower_lsp::lsp_types::*;

pub struct AnalysisResults {
    pub diagnostics: Vec<Diagnostic>,
    pub program: Option<Program>,
    pub analyzer: EntropicAnalyzer,
    pub source: Option<String>,
}

pub fn analyze(text: &str, _filename: &str) -> AnalysisResults {
    let mut diagnostics = Vec::new();
    let mut program_opt = None;
    let mut analyzer = EntropicAnalyzer::new();
    let source_text = Some(text.to_string());

    match parse_ictl(text) {
        Ok(program) => {
            program_opt = Some(program.clone());
            if let Err(err) =
                analyzer.analyze_program_with_source(&program, text, _filename)
            {
                diagnostics.push(Diagnostic {
                    range: Range {
                        start: Position {
                            line: err.line.unwrap_or(1) as u32 - 1,
                            character: err.column.unwrap_or(1) as u32 - 1,
                        },
                        end: Position {
                            line: err.line.unwrap_or(1) as u32 - 1,
                            character: (err.column.unwrap_or(1) + 10) as u32 - 1, // Suggestion
                        },
                    },
                    severity: Some(DiagnosticSeverity::ERROR),
                    message: format!("{}", err),
                    ..Diagnostic::default()
                });
            }
        }
        Err(err) => {
            // Pest error to diagnostic (simplified)
            diagnostics.push(Diagnostic {
                range: Range::default(),
                severity: Some(DiagnosticSeverity::ERROR),
                message: format!("Parse error: {}", err),
                ..Diagnostic::default()
            });
        }
    }

    AnalysisResults {
        diagnostics,
        program: program_opt,
        analyzer,
        source: source_text,
    }
}
