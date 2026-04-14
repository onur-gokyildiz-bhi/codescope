use tower_lsp::{LspService, Server};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .init();

    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();

    let (service, socket) = LspService::new(codescope_lsp::Backend::new);
    Server::new(stdin, stdout, socket).serve(service).await;

    Ok(())
}
