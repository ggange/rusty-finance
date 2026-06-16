# backtesting

A Rust library for running trading strategy backtests against historical OHLCV price data.

## Modules

| Module | Contents |
|--------|----------|
| `data` | `Candle` struct, `DataSource` trait, `CSVDataSource` |
| `strategy` | `Strategy` trait, `Signal` enum, `ma` and `rsi` sub-modules |
| `engine` | `BacktestEngine<S>`, `BacktestResult` |
| `portfolio` | `Portfolio`, `EquityPoint`, `TradeRecord` |
| `metrics` | `Metrics` — total return, max drawdown, Sharpe ratio |

## Running Tests

```bash
cargo test
```

Runs 29 unit tests and 7 integration tests. Integration tests load CSV fixtures from
`../data/fixtures/synthetic_30.csv` and require no external data.

## Implementing a Custom Strategy

```rust
use backtesting::strategy::{Signal, Strategy};
use backtesting::data::Candle;

struct AlwaysBuy;

impl Strategy for AlwaysBuy {
    fn on_bar(&mut self, _candle: &Candle) -> Signal {
        Signal::Buy
    }
}
```

Then pass it to `BacktestEngine::new(AlwaysBuy, portfolio)`.

## CSV Format

Headers must be exactly: `Date,Open,High,Low,Close,Volume`

- `Date`: `YYYY-MM-DD`
- `Open`, `High`, `Low`, `Close`: floating-point prices
- `Volume`: unsigned integer

## TDD Workflow

For every new strategy or feature:

1. **RED** — write a failing test in `#[cfg(test)] mod tests` or `tests/`.
2. **GREEN** — implement the minimum code to make it pass (`cargo test`).
3. **REFACTOR** — improve without breaking tests.

All public items must have a `///` doc comment before merging.
