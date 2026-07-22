#[cfg(feature = "use_mimalloc")]
#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

use mq_web_api::{
    Config,
    banner::print_banner,
    server::{init_tracing, start_server},
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config = Config::from_env();
    print_banner(&config);
    init_tracing(&config);
    start_server(config).await
}
