use crate::analysis_worker::AnalysisResults;
use dashmap::DashMap;
use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::*;
use tower_lsp::{Client, LanguageServer};

pub struct Backend {
    client: Client,
    document_map: DashMap<Url, String>,
    analysis_cache: DashMap<Url, AnalysisResults>,
}

impl Backend {
    pub fn new(client: Client) -> Self {
        Self {
            client,
            document_map: DashMap::new(),
            analysis_cache: DashMap::new(),
        }
    }

    async fn on_change(&self, params: TextDocumentItem) {
        self.document_map
            .insert(params.uri.clone(), params.text.clone());
        self.validate_document(params.uri).await;
    }

    async fn validate_document(&self, uri: Url) {
        let text = self.document_map.get(&uri).unwrap().clone();
        let path = uri.to_file_path().unwrap_or_default();
        let path_str = path.to_string_lossy().to_string();

        let results = crate::analysis_worker::analyze(&text, &path_str);

        // Publish diagnostics
        let diagnostics = results.diagnostics.clone();
        self.client
            .publish_diagnostics(uri.clone(), diagnostics, None)
            .await;

        // Cache results for hover/tokens
        self.analysis_cache.insert(uri, results);
    }
}

#[tower_lsp::async_trait]
impl LanguageServer for Backend {
    async fn initialize(&self, _: InitializeParams) -> Result<InitializeResult> {
        Ok(InitializeResult {
            server_info: Some(ServerInfo {
                name: "ictl-lsp".to_string(),
                version: Some("0.1.0".to_string()),
            }),
            capabilities: ServerCapabilities {
                text_document_sync: Some(TextDocumentSyncCapability::Kind(
                    TextDocumentSyncKind::FULL,
                )),
                hover_provider: Some(HoverProviderCapability::Simple(true)),
                semantic_tokens_provider: Some(
                    SemanticTokensServerCapabilities::SemanticTokensOptions(
                        SemanticTokensOptions {
                            work_done_progress_options:
                                WorkDoneProgressOptions::default(),
                            legend: SemanticTokensLegend {
                                token_types: vec![
                                    SemanticTokenType::VARIABLE,
                                    SemanticTokenType::COMMENT, // Used for shading
                                ],
                                token_modifiers: vec![],
                            },
                            range: Some(false),
                            full: Some(SemanticTokensFullOptions::Bool(true)),
                        },
                    ),
                ),
                inlay_hint_provider: Some(OneOf::Left(true)),
                ..ServerCapabilities::default()
            },
        })
    }

    async fn initialized(&self, _: InitializedParams) {
        self.client
            .log_message(MessageType::INFO, "ICTL LSP initialized!")
            .await;
    }

    async fn shutdown(&self) -> Result<()> {
        Ok(())
    }

    async fn did_open(&self, params: DidOpenTextDocumentParams) {
        self.on_change(params.text_document).await;
    }

    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        if let Some(change) = params.content_changes.into_iter().next() {
            self.on_change(TextDocumentItem {
                uri: params.text_document.uri,
                text: change.text,
                version: params.text_document.version,
                language_id: "ictl".to_string(),
            })
            .await;
        }
    }

    async fn hover(&self, params: HoverParams) -> Result<Option<Hover>> {
        crate::hover::handle_hover(params, &self.analysis_cache).await
    }

    async fn semantic_tokens_full(
        &self,
        params: SemanticTokensParams,
    ) -> Result<Option<SemanticTokensResult>> {
        crate::tokens::handle_tokens(params, &self.analysis_cache).await
    }

    async fn inlay_hint(
        &self,
        params: InlayHintParams,
    ) -> Result<Option<Vec<InlayHint>>> {
        crate::inlay_hints::handle_inlay_hints(params, &self.analysis_cache).await
    }

    async fn did_save(&self, params: DidSaveTextDocumentParams) {
        self.validate_document(params.text_document.uri).await;
    }
}
