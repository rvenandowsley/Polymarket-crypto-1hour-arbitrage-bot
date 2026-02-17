use anyhow::Result;
use chrono::{DateTime, Datelike, FixedOffset, Timelike, Utc};
use polymarket_client_sdk::gamma::{Client, types::request::MarketsRequest};
use polymarket_client_sdk::types::{B256, U256};
use tracing::{info, warn};

#[derive(Debug, Clone)]
pub struct MarketInfo {
    pub market_id: B256,
    pub slug: String,
    pub yes_token_id: U256,
    pub no_token_id: U256,
    pub title: String,
    pub end_date: DateTime<Utc>,
    pub crypto_symbol: String,
}

pub struct MarketDiscoverer {
    gamma_client: Client,
    crypto_symbols: Vec<String>,
}

impl MarketDiscoverer {
    pub fn new(crypto_symbols: Vec<String>) -> Self {
        Self {
            gamma_client: Client::default(),
            crypto_symbols,
        }
    }

    /// 计算当前1小时窗口的开始时间戳（基于ET时间）
    /// 窗口开始时间：每小时整点（例如3am开始，4am结束）
    pub fn calculate_current_window_timestamp(now: DateTime<Utc>) -> i64 {
        // 将UTC时间转换为ET时间（ET = UTC-5或UTC-4，取决于夏令时）
        // 简化处理：使用UTC-5（EST）作为基准，实际应用中可能需要更精确的DST处理
        let et_offset = FixedOffset::east_opt(-5 * 3600).unwrap();
        let et_time = now.with_timezone(&et_offset);
        
        // 构建当前小时窗口开始时间（分钟和秒都设为0）
        let target_time = et_time
            .with_minute(0)
            .and_then(|t| t.with_second(0))
            .and_then(|t| t.with_nanosecond(0))
            .unwrap_or(et_time);

        // 转换回UTC时间戳
        target_time.with_timezone(&Utc).timestamp()
    }

    /// 计算下一个1小时窗口的开始时间戳（基于ET时间）
    /// 窗口开始时间：每小时整点（例如3am开始，4am结束）
    pub fn calculate_next_window_timestamp(now: DateTime<Utc>) -> i64 {
        // 将UTC时间转换为ET时间
        let et_offset = FixedOffset::east_opt(-5 * 3600).unwrap();
        let et_time = now.with_timezone(&et_offset);
        
        // 如果当前时间正好是整点且秒数为0，使用当前小时，否则使用下一个小时
        let target_hour = if et_time.minute() == 0 && et_time.second() == 0 {
            et_time.hour()
        } else {
            et_time.hour() + 1
        };

        // 处理小时溢出（超过23点）
        let (final_hour, day_adjustment) = if target_hour >= 24 {
            (target_hour - 24, 1)
        } else {
            (target_hour, 0)
        };

        // 构建目标时间
        let mut target_time = et_time
            .with_hour(final_hour)
            .and_then(|t| t.with_minute(0))
            .and_then(|t| t.with_second(0))
            .and_then(|t| t.with_nanosecond(0))
            .unwrap_or(et_time);

        // 如果需要调整天数
        if day_adjustment > 0 {
            target_time = target_time + chrono::Duration::days(day_adjustment);
        }

        // 转换回UTC时间戳
        target_time.with_timezone(&Utc).timestamp()
    }

    /// 将UTC时间戳转换为ET时间的slug格式
    /// 格式：[月]-[天]-[时][am或pm]-et
    /// 例如：january-16-3am-et
    fn timestamp_to_slug_format(timestamp: i64) -> String {
        let et_offset = FixedOffset::east_opt(-5 * 3600).unwrap();
        let utc_time = DateTime::from_timestamp(timestamp, 0)
            .unwrap_or_else(|| Utc::now());
        let et_time = utc_time.with_timezone(&et_offset);

        // 月份名称
        let month_names = [
            "january", "february", "march", "april", "may", "june",
            "july", "august", "september", "october", "november", "december"
        ];
        let month = month_names.get((et_time.month0()) as usize)
            .unwrap_or(&"january");

        // 日期
        let day = et_time.day();

        // 12小时制时间和am/pm
        let hour_24 = et_time.hour();
        let (hour_12, am_pm) = if hour_24 == 0 {
            (12, "am")
        } else if hour_24 < 12 {
            (hour_24, "am")
        } else if hour_24 == 12 {
            (12, "pm")
        } else {
            (hour_24 - 12, "pm")
        };

        format!("{}-{}-{}{}-et", month, day, hour_12, am_pm)
    }

    /// 生成市场slug列表
    /// 格式：[币种]-up-or-down-[月]-[天]-[时][am或pm]-et
    /// 例如：bitcoin-up-or-down-january-16-3am-et
    pub fn generate_market_slugs(&self, timestamp: i64) -> Vec<String> {
        let time_suffix = Self::timestamp_to_slug_format(timestamp);
        self.crypto_symbols
            .iter()
            .map(|symbol| format!("{}-up-or-down-{}", symbol, time_suffix))
            .collect()
    }

    /// 获取指定时间戳的1小时市场
    pub async fn get_markets_for_timestamp(&self, timestamp: i64) -> Result<Vec<MarketInfo>> {
        // 生成所有加密货币的slug
        let slugs = self.generate_market_slugs(timestamp);

        info!(timestamp, slug_count = slugs.len(), "查询市场");

        // 使用Gamma API批量查询
        let request = MarketsRequest::builder()
            .slug(slugs.clone())
            .build();

        match self.gamma_client.markets(&request).await {
            Ok(markets) => {
                // 过滤并解析市场
                let valid_markets: Vec<MarketInfo> = markets
                    .into_iter()
                    .filter_map(|market| self.parse_market(market))
                    .collect();

                info!(count = valid_markets.len(), "找到符合条件的市场");
                Ok(valid_markets)
            }
            Err(e) => {
                warn!(error = %e, timestamp = timestamp, "查询市场失败，可能市场尚未创建");
                Ok(Vec::new())
            }
        }
    }

    /// 解析市场信息，提取YES和NO的token_id
    fn parse_market(&self, market: polymarket_client_sdk::gamma::types::response::Market) -> Option<MarketInfo> {
        // 检查市场是否活跃、启用订单簿且接受订单
        if !market.active.unwrap_or(false) 
           || !market.enable_order_book.unwrap_or(false)
           || !market.accepting_orders.unwrap_or(false) {
            return None;
        }

        // 检查outcomes是否为["Up", "Down"]
        let outcomes = market.outcomes.as_ref()?;

        if outcomes.len() != 2 
           || !outcomes.contains(&"Up".to_string()) 
           || !outcomes.contains(&"Down".to_string()) {
            return None;
        }

        // 获取clobTokenIds
        let token_ids = market.clob_token_ids.as_ref()?;

        if token_ids.len() != 2 {
            return None;
        }

        // 第一个是"Up"的token_id，第二个是"Down"的token_id
        let yes_token_id = token_ids[0];
        let no_token_id = token_ids[1];

        // 获取conditionId
        let market_id = market.condition_id?;

        // 从slug中提取加密货币符号
        let slug = market.slug.as_ref()?;
        let crypto_symbol = slug
            .split('-')
            .next()
            .unwrap_or("")
            .to_string();

        // 获取endDate
        let end_date = market.end_date?;

        Some(MarketInfo {
            market_id,
            slug: slug.clone(),
            yes_token_id,
            no_token_id,
            title: market.question.unwrap_or_default(),
            end_date,
            crypto_symbol,
        })
    }
}
