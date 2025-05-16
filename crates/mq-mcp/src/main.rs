pub mod server;

#[tokio::main]
async fn main() -> miette::Result<()> {
    server::start().await
}
