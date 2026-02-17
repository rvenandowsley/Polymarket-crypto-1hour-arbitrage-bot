# poly_1hour_bot

[English](README.md) | **中文**

面向 [Polymarket](https://polymarket.com) 加密货币「涨跌」1 小时市场（美东时间 ET）的 Rust 套利机器人。监控订单簿、检测 YES+NO 价差套利机会、通过 CLOB API 下单，并可定时对可赎回持仓执行 merge。

---

### TG联系方式：[@polyboy123](https://t.me/polyboy123)

下面是实盘收益，不到一天就赚了三十多USDC
<img width="1306" height="838" alt="image" src="https://github.com/user-attachments/assets/d7b33c69-fac7-4b58-a302-9fabd884a563" />



## 功能

- **市场发现**：按币种与 ET 时间窗口，从 Gamma API 拉取「涨/跌」每小时市场（如 `bitcoin-up-or-down-january-16-3am-et`）。
- **订单簿监控**：订阅 CLOB 订单簿，在 `yes_ask + no_ask < 1` 时判定套利机会。
- **套利执行**：下 YES、NO 双单（GTC/GTD/FOK/FAK），可配置滑点、单笔上限与执行价差。
- **风险管理**：跟踪敞口、遵守 `RISK_MAX_EXPOSURE_USDC`，可选对冲监控（当前对冲逻辑已关闭）。
- **Merge 任务**：定时拉取持仓，对 YES、NO 双边都持仓的市场执行 `merge_max` 赎回（需配置 `POLYMARKET_PROXY_ADDRESS` 与 `MERGE_INTERVAL_MINUTES`）。

---

## 试用
### 暂时只支持Linux,最好是ubuntu24
1. 下载release中的试用包：poly_1h_bot.zip
2. 放到云服务器上面，需要确保所在地域能够被polymarket允许交易
3. 配置好.env中前面的几个空白参数，参数由polymarket官网导出
4. 前台运行：`./poly_1h_bot`
5. 后台运行：`nohup ./poly_1h_bot > /dev/null 2>&1 &`

## 安装

### 环境要求

- **Rust 与 Cargo** 1.70+（Rust 编译器与 Cargo 包管理器，见下方安装说明）
- **许可证文件**：项目根目录下有效的 `license.key`

#### 安装 Rust 和 Cargo

Rust 通过 [rustup](https://rustup.rs) 安装，会同时安装 `rustc`（编译器）和 `cargo`（包管理器）。

**Linux / macOS：**

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

按提示选择默认安装（输入 `1` 回车）。安装完成后执行：

```bash
source $HOME/.cargo/env
```

**Windows：**

1. 下载 [rustup-init.exe](https://win.rustup.rs/x86_64)
2. 运行并按提示完成安装
3. 安装完成后重启终端

**验证安装：**

```bash
rustc --version   # 应显示 rustc 1.70 或更高
cargo --version   # 应显示 cargo 版本
```

### 安装步骤

1. **克隆仓库**

   ```bash
   git clone https://github.com/rvenandowsley/Polymarket-crypto-1hour-arbitrage-bot
   cd Polymarket-crypto-1hour-arbitrage-bot
   ```

2. **从模板创建 `.env`**

   ```bash
   cp .env.example .env
   ```

3. **编辑 `.env`** 并填写必填项：
   - `POLYMARKET_PRIVATE_KEY`（必填）：64 位十六进制私钥
   - `POLYMARKET_PROXY_ADDRESS`（启用 merge 时必填）：Polymarket 代理钱包地址
   - `POLY_BUILDER_API_KEY`、`POLY_BUILDER_SECRET`、`POLY_BUILDER_PASSPHRASE`（启用 merge 时必填）

4. **将 `license.key` 放入项目根目录**（或设置 `POLY_15MIN_BOT_LICENSE` 指向其路径）

5. **编译并运行**

   ```bash
   cargo build --release
   cargo run --release
   ```

---

## 环境要求

- **Rust** 1.70+（2021 edition）
- **配置**：项目根目录的 `.env` 文件（见 [配置说明](#配置说明)）

---

## 配置说明

在项目根目录创建 `.env`（可参考 `.env.example`）。环境变量说明：

| 变量名 | 必填 | 说明 |
|--------|------|------|
| `POLYMARKET_PRIVATE_KEY` | 是 | 64 位十六进制私钥（不带 `0x`），EOA 或 Proxy 对应私钥。 |
| `POLYMARKET_PROXY_ADDRESS` | 否* | 代理钱包地址（Email/Magic 或 Browser Wallet）。启用 merge 任务时必填。 |
| `MIN_PROFIT_THRESHOLD` | 否 | 套利检测最低利润率，默认 `0.001`。 |
| `MAX_ORDER_SIZE_USDC` | 否 | 单笔最大下单量（USDC），默认 `100.0`。 |
| `CRYPTO_SYMBOLS` | 否 | 币种列表，逗号分隔，如 `btc,eth,xrp,sol`，默认 `btc,eth,xrp,sol`。 |
| `MARKET_REFRESH_ADVANCE_SECS` | 否 | 提前多少秒刷新下一窗口市场，默认 `5`。 |
| `RISK_MAX_EXPOSURE_USDC` | 否 | 最大敞口上限（USDC），默认 `1000.0`。 |
| `RISK_IMBALANCE_THRESHOLD` | 否 | 风险不平衡阈值，默认 `0.1`。 |
| `HEDGE_TAKE_PROFIT_PCT` | 否 | 对冲止盈百分比，默认 `0.05`。 |
| `HEDGE_STOP_LOSS_PCT` | 否 | 对冲止损百分比，默认 `0.05`。 |
| `ARBITRAGE_EXECUTION_SPREAD` | 否 | 当 `yes+no <= 1 - spread` 时执行套利，默认 `0.01`。 |
| `SLIPPAGE` | 否 | `"first,second"` 或单个值，默认 `0,0.01`。 |
| `GTD_EXPIRATION_SECS` | 否 | GTD 订单过期时间（秒），默认 `300`。 |
| `ARBITRAGE_ORDER_TYPE` | 否 | `GTC` / `GTD` / `FOK` / `FAK`，默认 `GTD`。 |
| `STOP_ARBITRAGE_BEFORE_END_MINUTES` | 否 | 市场结束前 N 分钟停止套利；`0` 表示不限制，默认 `0`。 |
| `MERGE_INTERVAL_MINUTES` | 否 | Merge 执行间隔（分钟）；`0` 表示不启用，默认 `0`。 |
| `MIN_YES_PRICE_THRESHOLD` | 否 | 仅当 YES 价格 ≥ 此值时才套利；`0` 表示不限制，默认 `0`。 |

---

## 构建与运行

```bash
cargo build --release
cargo run --release
```

可通过 `RUST_LOG` 控制日志级别（如 `RUST_LOG=info` 或 `RUST_LOG=debug`）。

### 使用说明

- 程序初始化完成后进入主循环，运行前请确认 `.env` 配置正确。
- 启用 merge 功能需设置 `POLYMARKET_PROXY_ADDRESS` 和 `MERGE_INTERVAL_MINUTES`。
- 建议在 `screen` 或 `tmux` 等稳定环境中运行，以便长时间运行。

---

## 测试用二进制

| 二进制 | 用途 |
|--------|------|
| `test_merge` | 对指定市场执行 merge；需 `POLYMARKET_PRIVATE_KEY`、`POLYMARKET_PROXY_ADDRESS`。 |
| `test_order` | 测试下单。 |
| `test_positions` | 拉取持仓；需 `POLYMARKET_PROXY_ADDRESS`。 |
| `test_price` | 价格/订单簿相关测试。 |
| `test_trade` | 交易执行测试。 |

运行示例：

```bash
cargo run --release --bin test_merge
cargo run --release --bin test_positions
# 其他同理
```

---

## 项目结构

```
src/
├── main.rs           # 入口、merge 任务、主循环（订单簿 + 套利）
├── config.rs         # 从环境变量加载配置
├── lib.rs            # 库入口（merge、positions）
├── merge.rs          # Merge 逻辑
├── positions.rs      # 持仓拉取
├── market/           # 市场发现、调度
├── monitor/          # 订单簿、套利检测
├── risk/             # 风险管理、对冲监控、恢复
├── trading/          # 执行器、订单
└── bin/              # test_merge、test_order、test_positions 等
```

---

## 免责声明

本机器人对接真实市场与资金，请自行承担使用风险。使用前请充分理解配置项、风险限额及 Polymarket 相关条款。
