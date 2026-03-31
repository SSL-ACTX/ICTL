mod analysis_worker;
mod cli;
mod hover;
mod inlay_hints;
mod server;
mod tokens;

use cli::{parse_args, usage, AppMode};
use server::Backend;
use tower_lsp::{LspService, Server};

#[tokio::main]
async fn main() {
    match parse_args(std::env::args().map(|a| a.into())) {
        AppMode::Help => {
            println!("{}", usage("ictl-lsp"));
            return;
        }
        AppMode::Version => {
            println!("ictl-lsp {}", env!("CARGO_PKG_VERSION"));
            return;
        }
        AppMode::Serve => {}
    }

    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();

    let (service, socket) = LspService::new(|client| Backend::new(client));
    Server::new(stdin, stdout, socket).serve(service).await;
}
