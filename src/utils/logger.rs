use anyhow::Result;
use std::fs::File;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

pub fn init_logger() -> Result<()> {
    // 设置默认日志级别为 info，如果没有设置 RUST_LOG 环境变量
    // 屏蔽 polymarket SDK 的 serde unknown field 警告（如 feeType）
    let filter_str = std::env::var("RUST_LOG").unwrap_or_else(|_| "info".to_string());
    let filter_str = if filter_str.contains("polymarket_client_sdk") {
        filter_str
    } else {
        format!("{},polymarket_client_sdk::serde_helpers=error", filter_str)
    };
    let env_filter = EnvFilter::try_new(&filter_str).unwrap_or_else(|_| EnvFilter::new("info"));
    
    if let Ok(path) = std::env::var("LOG_FILE") {
        let file = File::create(path)?;
        tracing_subscriber::registry()
            .with(env_filter)
            .with(
                tracing_subscriber::fmt::layer()
                    .with_writer(file)
                    .with_ansi(false),
            )
            .init();
    } else {
        tracing_subscriber::fmt()
            .with_env_filter(env_filter)
            .init();
    }

    Ok(())
}
