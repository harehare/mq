use mq_web_api::{
    Config,
    server::{init_tracing, start_server},
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config = Config::from_env();
    init_tracing(&config);
    start_server(config).await
}
