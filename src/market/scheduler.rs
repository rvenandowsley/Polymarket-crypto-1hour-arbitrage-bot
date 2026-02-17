use anyhow::Result;
use chrono::{DateTime, Utc};
use std::time::Duration;
use tokio::time::sleep;
use tracing::{error, info, warn};

use super::discoverer::{MarketDiscoverer, MarketInfo};

pub struct MarketScheduler {
    discoverer: MarketDiscoverer,
    refresh_advance_secs: u64,
}

impl MarketScheduler {
    pub fn new(discoverer: MarketDiscoverer, refresh_advance_secs: u64) -> Self {
        Self {
            discoverer,
            refresh_advance_secs,
        }
    }

    /// 计算到下一个1小时窗口的等待时间
    pub fn calculate_wait_time(&self, now: DateTime<Utc>) -> Duration {
        let next_window_ts = MarketDiscoverer::calculate_next_window_timestamp(now);
        let next_window = DateTime::from_timestamp(next_window_ts, 0)
            .expect("Invalid timestamp");

        // 提前几秒查询，确保市场已创建
        let wait_duration = next_window
            .signed_duration_since(now)
            .to_std()
            .unwrap_or(Duration::ZERO)
            .saturating_sub(Duration::from_secs(self.refresh_advance_secs));

        wait_duration.max(Duration::ZERO)
    }

    /// 立即获取当前窗口的市场，如果失败则等待下一个窗口
    pub async fn get_markets_immediately_or_wait(&self) -> Result<Vec<MarketInfo>> {
        // 首先尝试获取当前窗口的市场
        let now = Utc::now();
        let current_timestamp = MarketDiscoverer::calculate_current_window_timestamp(now);
        let next_timestamp = MarketDiscoverer::calculate_next_window_timestamp(now);
        
        // 如果当前窗口和下一个窗口相同（正好在窗口开始时间），只查询一次
        if current_timestamp == next_timestamp {
            return self.wait_for_next_window().await;
        }

                info!("尝试获取当前窗口的市场");
        match self.discoverer.get_markets_for_timestamp(current_timestamp).await {
            Ok(markets) => {
                if !markets.is_empty() {
                    info!(count = markets.len(), "发现当前窗口的市场");
                    return Ok(markets);
                }
                // 当前窗口没有市场，等待下一个窗口
                info!("当前窗口没有市场，等待下一个窗口");
                self.wait_for_next_window().await
            }
            Err(e) => {
                warn!(error = %e, "获取当前窗口市场失败，等待下一个窗口");
                self.wait_for_next_window().await
            }
        }
    }

    /// 等待到下一个1小时窗口开始，并获取市场
    pub async fn wait_for_next_window(&self) -> Result<Vec<MarketInfo>> {
        loop {
            let wait_time = self.calculate_wait_time(Utc::now());
            if wait_time > Duration::ZERO {
                info!(
                    wait_secs = wait_time.as_secs(),
                    "等待下一个1小时窗口"
                );
                sleep(wait_time).await;
            }

            // 查询当前窗口的市场
            let now = Utc::now();
            let timestamp = MarketDiscoverer::calculate_current_window_timestamp(now);
            match self.discoverer.get_markets_for_timestamp(timestamp).await {
                Ok(markets) => {
                    if !markets.is_empty() {
                        info!(count = markets.len(), "发现新市场");
                        return Ok(markets);
                    }
                    // 如果市场还未创建，等待一段时间后重试
                    info!("市场尚未创建，等待重试...");
                    sleep(Duration::from_secs(2)).await;
                }
                Err(e) => {
                    error!(error = %e, "获取市场失败，重试...");
                    sleep(Duration::from_secs(2)).await;
                }
            }
        }
    }
}
