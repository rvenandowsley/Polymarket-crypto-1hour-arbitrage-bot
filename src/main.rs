mod config;
mod market;
mod monitor;
mod risk;
mod trading;
mod utils;

use poly_15min_bot::merge;
use poly_15min_bot::positions::{get_positions, Position};

use anyhow::Result;
use futures::StreamExt;
use rust_decimal_macros::dec;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::Duration;
use tokio::time::sleep;
use tracing::{debug, error, info, warn};
use polymarket_client_sdk::types::{Address, B256};

use crate::config::Config;
use crate::market::{MarketDiscoverer, MarketInfo, MarketScheduler};
use crate::monitor::{ArbitrageDetector, OrderBookMonitor};
use crate::risk::{HedgeMonitor, RiskManager};
use crate::trading::TradingExecutor;

/// 从持仓中筛出 **YES 和 NO 都持仓** 的 condition_id（outcome_index 0 与 1 均存在且 size>0），
/// 仅这些市场才能 merge；单边持仓直接跳过。
fn condition_ids_with_both_sides(positions: &[Position]) -> Vec<B256> {
    let mut by_condition: HashMap<B256, HashSet<i32>> = HashMap::new();
    for p in positions {
        if p.size <= dec!(0) {
            continue;
        }
        by_condition
            .entry(p.condition_id)
            .or_default()
            .insert(p.outcome_index);
    }
    by_condition
        .into_iter()
        .filter(|(_, indices)| indices.contains(&0) && indices.contains(&1))
        .map(|(c, _)| c)
        .collect()
}

/// 定时 Merge 任务：每 interval_minutes 分钟拉取**持仓**，仅对 YES+NO 双边都持仓的市场 **串行**执行 merge_max，
/// 单边持仓跳过；每笔之间间隔、对 RPC 限速做一次重试。在独立 spawn 中运行，不阻塞订单簿。
async fn run_merge_task(interval_minutes: u64, proxy: Address, private_key: String) {
    let interval = Duration::from_secs(interval_minutes * 60);
    /// 每笔 merge 之间间隔，降低 RPC  bursts
    const DELAY_BETWEEN_MERGES: Duration = Duration::from_secs(30);
    /// 遇限速时等待后重试的时长（略大于 "retry in 10s"）
    const RATE_LIMIT_BACKOFF: Duration = Duration::from_secs(12);

    loop {
        let condition_ids = match get_positions().await {
            Ok(positions) => condition_ids_with_both_sides(&positions),
            Err(e) => {
                warn!(error = %e, "获取持仓失败，跳过本轮回 merge");
                sleep(interval).await;
                continue;
            }
        };

        if condition_ids.is_empty() {
            debug!("本轮回 merge: 无满足 YES+NO 双边持仓的市场");
        } else {
            info!(
                count = condition_ids.len(),
                "本轮回 merge: 共 {} 个市场满足 YES+NO 双边持仓",
                condition_ids.len()
            );
        }

        for (i, &condition_id) in condition_ids.iter().enumerate() {
            if i > 0 {
                sleep(DELAY_BETWEEN_MERGES).await;
            }
            let mut result = merge::merge_max(condition_id, proxy, &private_key, None).await;
            if result.is_err() {
                let msg = result.as_ref().unwrap_err().to_string();
                if msg.contains("rate limit") || msg.contains("retry in") {
                    warn!(condition_id = %condition_id, "RPC 限速，等待 {}s 后重试一次", RATE_LIMIT_BACKOFF.as_secs());
                    sleep(RATE_LIMIT_BACKOFF).await;
                    result = merge::merge_max(condition_id, proxy, &private_key, None).await;
                }
            }
            match result {
                Ok(tx) => {
                    info!("Merge 完成 | condition_id={:#x}", condition_id);
                    info!("  tx={}", tx);
                }
                Err(e) => {
                    let msg = e.to_string();
                    if msg.contains("无可用份额") {
                        debug!(condition_id = %condition_id, "跳过 merge: 无可用份额");
                    } else {
                        warn!(condition_id = %condition_id, error = %e, "Merge 失败");
                    }
                }
            }
        }

        sleep(interval).await;
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    // 初始化日志
    utils::logger::init_logger()?;

    tracing::info!("Polymarket 1小时套利机器人启动");

    // 加载配置
    let config = Config::from_env()?;
    tracing::info!("配置加载完成");

    // 初始化组件（暂时不使用，主循环已禁用）
    let _discoverer = MarketDiscoverer::new(config.crypto_symbols.clone());
    let _scheduler = MarketScheduler::new(_discoverer, config.market_refresh_advance_secs);
    let _detector = ArbitrageDetector::new(config.min_profit_threshold);
    
    // 验证私钥格式
    info!("正在验证私钥格式...");
    use alloy::signers::local::LocalSigner;
    use polymarket_client_sdk::POLYGON;
    use std::str::FromStr;
    
    let _signer_test = LocalSigner::from_str(&config.private_key)
        .map_err(|e| anyhow::anyhow!("私钥格式无效: {}", e))?;
    info!("私钥格式验证通过");
    
    // 初始化交易执行器（需要认证）
    info!("正在初始化交易执行器（需要API认证）...");
    if let Some(ref proxy) = config.proxy_address {
        info!(proxy_address = %proxy, "使用Proxy签名类型（Email/Magic或Browser Wallet）");
    } else {
        info!("使用EOA签名类型（直接交易）");
    }
    info!("注意：如果看到'Could not create api key'警告，这是正常的。SDK会先尝试创建新API key，失败后会自动使用派生方式，认证仍然会成功。");
    let executor = match TradingExecutor::new(
        config.private_key.clone(),
        config.max_order_size_usdc,
        config.proxy_address,
        config.slippage,
        config.gtd_expiration_secs,
        config.arbitrage_order_type.clone(),
    ).await {
        Ok(exec) => {
            info!("交易执行器认证成功（可能使用了派生API key）");
            Arc::new(exec)
        }
        Err(e) => {
            error!(error = %e, "交易执行器认证失败！无法继续运行。");
            error!("请检查：");
            error!("  1. POLYMARKET_PRIVATE_KEY 环境变量是否正确设置");
            error!("  2. 私钥格式是否正确（应该是64字符的十六进制字符串，不带0x前缀）");
            error!("  3. 网络连接是否正常");
            error!("  4. Polymarket API服务是否可用");
            return Err(anyhow::anyhow!("认证失败，程序退出: {}", e));
        }
    };

    // 创建CLOB客户端用于风险管理（需要认证）
    info!("正在初始化风险管理客户端（需要API认证）...");
    use alloy::signers::Signer;
    use polymarket_client_sdk::clob::{Client, Config as ClobConfig};
    use polymarket_client_sdk::clob::types::SignatureType;

    let signer_for_risk = LocalSigner::from_str(&config.private_key)?
        .with_chain_id(Some(POLYGON));
    let clob_config = ClobConfig::builder().use_server_time(true).build();
    let mut auth_builder_risk = Client::new("https://clob.polymarket.com", clob_config)?
        .authentication_builder(&signer_for_risk);
    
    // 如果提供了proxy_address，设置funder和signature_type
    if let Some(funder) = config.proxy_address {
        auth_builder_risk = auth_builder_risk
            .funder(funder)
            .signature_type(SignatureType::Proxy);
    }
    
    let clob_client = match auth_builder_risk.authenticate().await {
        Ok(client) => {
            info!("风险管理客户端认证成功（可能使用了派生API key）");
            client
        }
        Err(e) => {
            error!(error = %e, "风险管理客户端认证失败！无法继续运行。");
            error!("请检查：");
            error!("  1. POLYMARKET_PRIVATE_KEY 环境变量是否正确设置");
            error!("  2. 私钥格式是否正确");
            error!("  3. 网络连接是否正常");
            error!("  4. Polymarket API服务是否可用");
            return Err(anyhow::anyhow!("认证失败，程序退出: {}", e));
        }
    };
    
    let _risk_manager = Arc::new(RiskManager::new(clob_client.clone(), &config));
    
    // 创建对冲监测器（传入PositionTracker的Arc引用以更新风险敞口）
    // 对冲策略已暂时关闭，但保留hedge_monitor变量以备将来使用
    let position_tracker = _risk_manager.position_tracker();
    let _hedge_monitor = HedgeMonitor::new(
        clob_client,
        config.private_key.clone(),
        config.proxy_address.clone(),
        position_tracker,
    );

    // 验证认证是否真的成功 - 尝试一个简单的API调用
    info!("正在验证认证状态（通过API调用测试）...");
    match executor.verify_authentication().await {
        Ok(_) => {
            info!("✅ 认证验证成功，API调用正常");
        }
        Err(e) => {
            error!(error = %e, "❌ 认证验证失败！虽然authenticate()没有报错，但API调用失败。");
            error!("这表明认证实际上没有成功，可能是：");
            error!("  1. API密钥创建失败（看到'Could not create api key'警告）");
            error!("  2. 私钥对应的账户可能没有在Polymarket上注册");
            error!("  3. 账户可能被限制或暂停");
            error!("  4. 网络连接问题");
            error!("程序将退出，请解决认证问题后再运行。");
            return Err(anyhow::anyhow!("认证验证失败: {}", e));
        }
    }

    info!("✅ 所有组件初始化完成，认证验证通过");

    // 定时 Merge：每 N 分钟根据持仓执行 merge，仅对 YES+NO 双边都持仓的市场
    let merge_interval = config.merge_interval_minutes;
    if merge_interval > 0 {
        if let Some(proxy) = config.proxy_address {
            let private_key = config.private_key.clone();
            tokio::spawn(async move {
                run_merge_task(merge_interval, proxy, private_key).await;
            });
            info!(
                interval_minutes = merge_interval,
                "已启动定时 Merge 任务，每 {} 分钟根据持仓执行（仅 YES+NO 双边）",
                merge_interval
            );
        } else {
            warn!("MERGE_INTERVAL_MINUTES={} 但未设置 POLYMARKET_PROXY_ADDRESS，定时 Merge 已禁用", merge_interval);
        }
    } else {
        info!("定时 Merge 未启用（MERGE_INTERVAL_MINUTES=0），如需启用请在 .env 中设置 MERGE_INTERVAL_MINUTES 为正数，例如 5 或 15");
    }

    // 主循环已启用，开始监控和交易

    // 主循环 - 暂时禁用
    #[allow(unreachable_code)]
    loop {
        // 立即获取当前窗口的市场，如果失败则等待下一个窗口
        let markets = match _scheduler.get_markets_immediately_or_wait().await {
            Ok(markets) => markets,
            Err(e) => {
                error!(error = %e, "获取市场失败");
                sleep(Duration::from_secs(60)).await;
                continue;
            }
        };

        if markets.is_empty() {
            warn!("未找到任何市场，跳过当前窗口");
            continue;
        }

        // 初始化订单簿监控器
        let mut monitor = OrderBookMonitor::new();

        // 订阅所有市场
        for market in &markets {
            if let Err(e) = monitor.subscribe_market(market) {
                error!(error = %e, market_id = %market.market_id, "订阅市场失败");
            }
        }

        // 创建订单簿流
        let mut stream = match monitor.create_orderbook_stream() {
            Ok(stream) => stream,
            Err(e) => {
                error!(error = %e, "创建订单簿流失败");
                continue;
            }
        };

        info!(market_count = markets.len(), "开始监控订单簿");

        // 记录当前窗口的时间戳，用于检测周期切换
        use chrono::Utc;
        let current_window_timestamp = MarketDiscoverer::calculate_current_window_timestamp(Utc::now());

        // 创建市场ID到市场信息的映射
        let market_map: HashMap<B256, &MarketInfo> = markets.iter()
            .map(|m| (m.market_id, m))
            .collect();

        // 监控订单簿更新
        loop {
            tokio::select! {
                // 处理订单簿更新
                book_result = stream.next() => {
                    match book_result {
                        Some(Ok(book)) => {
                            // 处理订单簿更新
                            // 对冲策略已暂时关闭，买进单边不做任何处理
                            // if let Err(e) = hedge_monitor.check_and_execute(&book).await {
                            //     error!(error = %e, "对冲监测检查失败");
                            // }
                            
                            // 然后处理订单簿更新（book会被move）
                            if let Some(pair) = monitor.handle_book_update(book) {
                                // 打印完整的订单簿对信息
                                // 注意：asks数组是价格升序，最后一个是最高的卖一价
                                // bids数组是价格降序，最后一个是最低的买一价
                                let yes_best_ask = pair.yes_book.asks.last().map(|a| (a.price, a.size));
                                let no_best_ask = pair.no_book.asks.last().map(|a| (a.price, a.size));
                                
                                // 计算总价（用于套利判断）
                                let total_ask_price = yes_best_ask.and_then(|(p, _)| no_best_ask.map(|(np, _)| p + np));
                                
                                // 获取市场信息
                                let market_info = market_map.get(&pair.market_id);
                                let market_title = market_info.map(|m| m.title.as_str()).unwrap_or("未知市场");
                                let market_symbol = market_info.map(|m| m.crypto_symbol.as_str()).unwrap_or("");
                                
                                // 格式化市场名称
                                let market_display = if !market_symbol.is_empty() {
                                    format!("{}预测市场", market_symbol)
                                } else {
                                    market_title.to_string()
                                };
                                
                                // 格式化输出为紧凑的单行格式
                                let yes_info = yes_best_ask
                                    .map(|(p, s)| format!("Yes卖一价:{:.4} 份额:{}", p, s))
                                    .unwrap_or_else(|| "Yes卖一价:无".to_string());
                                
                                let no_info = no_best_ask
                                    .map(|(p, s)| format!("No卖一价:{:.4} 份额:{}", p, s))
                                    .unwrap_or_else(|| "No卖一价:无".to_string());
                                
                                // 检查是否有套利机会
                                let (prefix, spread_info) = total_ask_price
                                    .map(|t| {
                                        if t < dec!(1.0) {
                                            let profit_pct = (dec!(1.0) - t) * dec!(100.0);
                                            ("🚨套利机会", format!("总价:{:.4} 利润:{:.4}%", t, profit_pct))
                                        } else {
                                            ("📊", format!("总价:{:.4} (无套利)", t))
                                        }
                                    })
                                    .unwrap_or_else(|| ("📊", "无数据".to_string()));
                                
                                info!(
                                    "{} {} | {} | {} | {}",
                                    prefix,
                                    market_display,
                                    yes_info,
                                    no_info,
                                    spread_info
                                );
                                
                                // 保留原有的结构化日志用于调试（可选）
                                debug!(
                                    market_id = %pair.market_id,
                                    yes_token = %pair.yes_book.asset_id,
                                    no_token = %pair.no_book.asset_id,
                                    "订单簿对详细信息"
                                );

                                // 检测套利机会（监控阶段：只有当总价 <= 1 - 套利执行价差 时才执行套利）
                                use rust_decimal::Decimal;
                                let execution_threshold = dec!(1.0) - Decimal::try_from(config.arbitrage_execution_spread)
                                    .unwrap_or(dec!(0.01));
                                if let Some(total_price) = total_ask_price {
                                    if total_price <= execution_threshold {
                                        if let Some(opp) = _detector.check_arbitrage(
                                            &pair.yes_book,
                                            &pair.no_book,
                                            &pair.market_id,
                                        ) {
                                            // 检查 YES 价格是否达到阈值
                                            if config.min_yes_price_threshold > 0.0 {
                                                use rust_decimal::Decimal;
                                                let min_yes_price_decimal = Decimal::try_from(config.min_yes_price_threshold)
                                                    .unwrap_or(dec!(0.0));
                                                if opp.yes_ask_price < min_yes_price_decimal {
                                                    debug!(
                                                        "⏸️ YES价格未达到阈值，跳过套利执行 | 市场:{} | YES价格:{:.4} | 阈值:{:.4}",
                                                        market_display,
                                                        opp.yes_ask_price,
                                                        config.min_yes_price_threshold
                                                    );
                                                    continue; // 跳过这个套利机会
                                                }
                                            }
                                            
                                            // 检查是否接近市场结束时间（如果配置了停止时间）
                                            if config.stop_arbitrage_before_end_minutes > 0 {
                                                if let Some(market_info) = market_map.get(&pair.market_id) {
                                                    use chrono::Utc;
                                                    let now = Utc::now();
                                                    let time_until_end = market_info.end_date.signed_duration_since(now);
                                                    let minutes_until_end = time_until_end.num_minutes();
                                                    
                                                    if minutes_until_end <= config.stop_arbitrage_before_end_minutes as i64 {
                                                        debug!(
                                                            "⏰ 接近市场结束时间，跳过套利执行 | 市场:{} | 距离结束:{}分钟 | 停止阈值:{}分钟",
                                                            market_display,
                                                            minutes_until_end,
                                                            config.stop_arbitrage_before_end_minutes
                                                        );
                                                        continue; // 跳过这个套利机会
                                                    }
                                                }
                                            }
                                            
                                            // 计算订单成本（USD）
                                            // 使用套利机会中的实际可用数量，但不超过配置的最大订单大小
                                            use rust_decimal::Decimal;
                                            let max_order_size = Decimal::try_from(config.max_order_size_usdc).unwrap_or(dec!(100.0));
                                            let order_size = opp.yes_size.min(opp.no_size).min(max_order_size);
                                            let yes_cost = opp.yes_ask_price * order_size;
                                            let no_cost = opp.no_ask_price * order_size;
                                            let total_cost = yes_cost + no_cost;
                                            
                                            // 检查风险敞口限制
                                            let position_tracker = _risk_manager.position_tracker();
                                            let current_exposure = position_tracker.calculate_exposure();
                                            
                                            if position_tracker.would_exceed_limit(yes_cost, no_cost) {
                                                warn!(
                                                    "⚠️ 风险敞口超限，拒绝执行套利交易 | 市场:{} | 当前敞口:{:.2} USD | 订单成本:{:.2} USD | 限制:{:.2} USD",
                                                    market_display,
                                                    current_exposure,
                                                    total_cost,
                                                    position_tracker.max_exposure()
                                                );
                                                continue; // 跳过这个套利机会
                                            }
                                            
                                            info!(
                                                "⚡ 执行套利交易 | 市场:{} | 利润:{:.2}% | 下单数量:{}份 | 订单成本:{:.2} USD | 当前敞口:{:.2} USD",
                                                market_display,
                                                opp.profit_percentage,
                                                order_size,
                                                total_cost,
                                                current_exposure
                                            );
                                            
                                            // 克隆需要的变量到独立任务中
                                            let executor_clone = executor.clone();
                                            let risk_manager_clone = _risk_manager.clone();
                                            let opp_clone = opp.clone();
                                            
                                            // 使用 tokio::spawn 异步执行套利交易，不阻塞订单簿更新处理
                                            tokio::spawn(async move {
                                                // 执行套利交易
                                                match executor_clone.execute_arbitrage_pair(&opp_clone).await {
                                                    Ok(result) => {
                                                        // 先保存 pair_id，因为 result 会被移动
                                                        let pair_id = result.pair_id.clone();
                                                        
                                                        // 注册到风险管理器（传入价格信息以计算风险敞口）
                                                        risk_manager_clone.register_order_pair(
                                                            result,
                                                            opp_clone.market_id,
                                                            opp_clone.yes_token_id,
                                                            opp_clone.no_token_id,
                                                            opp_clone.yes_ask_price,
                                                            opp_clone.no_ask_price,
                                                        );

                                                        // 处理风险恢复
                                                        // 对冲策略已暂时关闭，买进单边不做任何处理
                                                        match risk_manager_clone.handle_order_pair(&pair_id).await {
                                                            Ok(action) => {
                                                                // 对冲策略已关闭，不再处理MonitorForExit和SellExcess
                                                                match action {
                                                                    crate::risk::recovery::RecoveryAction::None => {
                                                                        // 正常情况，无需处理
                                                                    }
                                                                    crate::risk::recovery::RecoveryAction::MonitorForExit { .. } => {
                                                                        info!("单边成交，但对冲策略已关闭，不做处理");
                                                                    }
                                                                    crate::risk::recovery::RecoveryAction::SellExcess { .. } => {
                                                                        info!("部分成交不平衡，但对冲策略已关闭，不做处理");
                                                                    }
                                                                    crate::risk::recovery::RecoveryAction::ManualIntervention { reason } => {
                                                                        warn!("需要手动干预: {}", reason);
                                                                    }
                                                                }
                                                            }
                                                            Err(e) => {
                                                                error!("风险处理失败: {}", e);
                                                            }
                                                        }
                                                    }
                                                    Err(e) => {
                                                        // 错误详情已在executor中记录，这里只记录简要信息
                                                        let error_msg = e.to_string();
                                                        // 提取简化的错误信息
                                                        if error_msg.contains("套利失败") {
                                                            // 错误信息已经格式化好了，直接使用
                                                            error!("{}", error_msg);
                                                        } else {
                                                            error!("执行套利交易失败: {}", error_msg);
                                                        }
                                                    }
                                                }
                                            });
                                        }
                                    }
                                }
                            }
                        }
                        Some(Err(e)) => {
                            error!(error = %e, "订单簿更新错误");
                            // 流错误，重新创建流
                            break;
                        }
                        None => {
                            warn!("订单簿流结束，重新创建");
                            break;
                        }
                    }
                }

                // 定期检查是否进入新的1小时窗口（每5秒检查一次）
                _ = sleep(Duration::from_secs(5)) => {
                    let now = Utc::now();
                    let new_window_timestamp = MarketDiscoverer::calculate_current_window_timestamp(now);
                    
                    // 如果当前窗口时间戳与记录的不同，说明已经进入新窗口
                    if new_window_timestamp != current_window_timestamp {
                        info!(
                            old_window = current_window_timestamp,
                            new_window = new_window_timestamp,
                            "检测到新的1小时窗口，准备取消旧订阅并切换到新窗口"
                        );
                        // 先drop stream以释放对monitor的借用，然后清理旧的订阅
                        drop(stream);
                        monitor.clear();
                        break;
                    }
                }
            }
        }

        // monitor 会在循环结束时自动 drop，无需手动清理
        info!("当前窗口监控结束，等待下一个窗口");
    }
}

