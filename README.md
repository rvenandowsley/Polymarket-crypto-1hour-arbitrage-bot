# poly_1hour_bot

**English** | [中文](README.zh-CN.md)

A Rust arbitrage bot for [Polymarket](https://polymarket.com) crypto “Up or Down” 1‑hour markets (ET). It monitors order books, detects YES+NO spread arbitrage opportunities, executes trades via the CLOB API, and can periodically merge redeemable positions.

---

### Telegram contact information: [@polyboy123](https://t.me/polyboy123)

<img width="1027" height="788" alt="image" src="https://github.com/user-attachments/assets/7ea3f755-5afa-4e4c-939d-6532e76cdac0" />


## Features

- **Market discovery**: Fetches “Up/Down” hourly markets (e.g. `bitcoin-up-or-down-january-16-3am-et`) from Gamma API by symbol and ET window.
- **Order book monitoring**: Subscribes to CLOB order books, detects when `yes_ask + no_ask < 1` (arbitrage opportunity).
- **Arbitrage execution**: Places YES and NO orders (GTC/GTD/FOK/FAK), with configurable slippage, size limits, and execution threshold.
- **Risk management**: Tracks exposure, enforces `RISK_MAX_EXPOSURE_USDC`, and optionally monitors hedges (hedge logic currently disabled).
- **Merge task**: Periodically fetches positions, and for markets where you hold both YES and NO, runs `merge_max` to redeem (requires `POLYMARKET_PROXY_ADDRESS` and `MERGE_INTERVAL_MINUTES`).

---

## Trial Use

1. Download the trial package from the release: poly_1h_bot.zip
2. Place it on a cloud server, ensuring your region is allowed to trade by PolyMarket.
3. Configure the first few blank parameters in the .env file. These parameters are exported from the PolyMarket website.
4. Run in the foreground: `./poly_1h_bot`
5. Run in the background: `nohup ./poly_1h_bot > /dev/null 2>&1 &`

## Installation

### Prerequisites

- **Rust & Cargo** 1.70+ (Rust compiler and Cargo package manager; see installation below)
- **License file**: Valid `license.key` in project root 

#### Install Rust and Cargo

Rust is installed via [rustup](https://rustup.rs), which installs both `rustc` (compiler) and `cargo` (package manager).

**Linux / macOS:**

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

Choose the default installation when prompted (press `1` then Enter). After installation, run:

```bash
source $HOME/.cargo/env
```

**Windows:**

1. Download [rustup-init.exe](https://win.rustup.rs/x86_64)
2. Run it and follow the prompts
3. Restart your terminal after installation

**Verify installation:**

```bash
rustc --version   # Should show rustc 1.70 or higher
cargo --version   # Should show cargo version
```

### Steps

1. **Clone the repository**

   ```bash
   git clone https://github.com/rvenandowsley/Polymarket-crypto-1hour-arbitrage-bot
   cd Polymarket-crypto-1hour-arbitrage-bot
   ```

2. **Create `.env` from template**

   ```bash
   cp .env.example .env
   ```

3. **Edit `.env`** and set required variables:
   - `POLYMARKET_PRIVATE_KEY` (required): 64‑char hex private key
   - `POLYMARKET_PROXY_ADDRESS` (required for merge): Your Polymarket proxy wallet address
   - `POLY_BUILDER_API_KEY`, `POLY_BUILDER_SECRET`, `POLY_BUILDER_PASSPHRASE` (required for merge)

4. **Place `license.key`** in the project root (or set `POLY_15MIN_BOT_LICENSE` to its path)

5. **Build and run**

   ```bash
   cargo build --release
   cargo run --release
   ```

---

## Requirements

- **Rust** 1.70+ (2021 edition)
- **Environment**: `.env` in project root (see [Configuration](#configuration)).

---

## Configuration

Create a `.env` file (see `.env.example` if available). Required and optional variables:

| Variable | Required | Description |
|----------|----------|-------------|
| `POLYMARKET_PRIVATE_KEY` | Yes | 64‑char hex private key (no `0x`). EOA or key for Proxy. |
| `POLYMARKET_PROXY_ADDRESS` | No* | Proxy wallet address (Email/Magic or Browser Wallet). Required for merge task. |
| `MIN_PROFIT_THRESHOLD` | No | Min profit ratio for arb detection (default `0.001`). |
| `MAX_ORDER_SIZE_USDC` | No | Max order size in USDC (default `100.0`). |
| `CRYPTO_SYMBOLS` | No | Comma‑separated symbols, e.g. `btc,eth,xrp,sol` (default `btc,eth,xrp,sol`). |
| `MARKET_REFRESH_ADVANCE_SECS` | No | Seconds before next window to refresh markets (default `5`). |
| `RISK_MAX_EXPOSURE_USDC` | No | Max exposure cap in USDC (default `1000.0`). |
| `RISK_IMBALANCE_THRESHOLD` | No | Imbalance threshold for risk (default `0.1`). |
| `HEDGE_TAKE_PROFIT_PCT` | No | Hedge take‑profit % (default `0.05`). |
| `HEDGE_STOP_LOSS_PCT` | No | Hedge stop‑loss % (default `0.05`). |
| `ARBITRAGE_EXECUTION_SPREAD` | No | Execute when `yes+no <= 1 - spread` (default `0.01`). |
| `SLIPPAGE` | No | `"first,second"` or single value (default `0,0.01`). |
| `GTD_EXPIRATION_SECS` | No | GTD order expiry in seconds (default `300`). |
| `ARBITRAGE_ORDER_TYPE` | No | `GTC` \| `GTD` \| `FOK` \| `FAK` (default `GTD`). |
| `STOP_ARBITRAGE_BEFORE_END_MINUTES` | No | Stop arb N minutes before market end; `0` = disabled (default `0`). |
| `MERGE_INTERVAL_MINUTES` | No | Merge interval in minutes; `0` = disabled (default `0`). |
| `MIN_YES_PRICE_THRESHOLD` | No | Only arb when YES price ≥ this; `0` = no filter (default `0`). |

---

## Build & Run

```bash
cargo build --release
cargo run --release
```

Logging can be controlled via `RUST_LOG` (e.g. `RUST_LOG=info` or `RUST_LOG=debug`).

### Usage notes

- The bot starts the main loop after initialization. Ensure `.env` is correctly configured before running.
- For merge functionality, `POLYMARKET_PROXY_ADDRESS` and `MERGE_INTERVAL_MINUTES` must be set.
- Run in a stable environment (e.g. `screen` or `tmux`) for long‑running sessions.

---

## Test binaries

| Binary | Purpose |
|--------|---------|
| `test_merge` | Run merge for a market; needs `POLYMARKET_PRIVATE_KEY`, `POLYMARKET_PROXY_ADDRESS`. |
| `test_order` | Test order placement. |
| `test_positions` | Fetch positions; needs `POLYMARKET_PROXY_ADDRESS`. |
| `test_price` | Price / order book checks. |
| `test_trade` | Trade execution tests. |

Run with:

```bash
cargo run --release --bin test_merge
cargo run --release --bin test_positions
# etc.
```

---

## Project structure

```
src/
├── main.rs           # Entrypoint, merge task, main loop (order book + arb)
├── config.rs         # Config from env
├── lib.rs            # Library root (merge, positions)
├── merge.rs          # Merge logic
├── positions.rs      # Position fetching
├── market/           # Discovery, scheduling
├── monitor/          # Order book, arbitrage detection
├── risk/             # Risk manager, hedge monitor, recovery
├── trading/          # Executor, orders
└── bin/              # test_merge, test_order, test_positions, ...
```

---

## Disclaimer

This bot interacts with real markets and real funds. Use at your own risk. Ensure you understand the config, risk limits, and Polymarket’s terms before running.
