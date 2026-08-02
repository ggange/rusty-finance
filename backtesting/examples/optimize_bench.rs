//! Timing harness for weight optimization against the real dataset catalog.
//!
//! Run from the repo root so the relative dataset paths resolve:
//!
//! ```text
//! cargo run --release -p backtesting --example optimize_bench
//! ```
//!
//! Dynamic policies re-solve at every rebalance date, so this is the number that
//! decides whether per-rebalance optimization is practical or merely possible.

use std::time::Instant;

use backtesting::data::{CSVDataSource, DataSource};
use backtesting::engine::FillTiming;
use backtesting::optimize::{Objective, OptimizerConfig};
use backtesting::portfolio::ExecutionCosts;
use backtesting::portfolio_backtest::{
    run_portfolio_with_policy, PortfolioAsset, RebalanceConfig, RebalanceFrequency, WeightPolicy,
};
use backtesting::strategy::rsi::RSI;

fn main() {
    let symbols = ["AAPL", "MSFT", "GOOG", "NVDA", "SPY"];

    let load = || -> Vec<PortfolioAsset> {
        symbols
            .iter()
            .map(|s| PortfolioAsset {
                symbol: s.to_string(),
                weight: 1.0,
                candles: CSVDataSource { file_path: format!("data/datasets/{s}.csv") }
                    .load()
                    .expect("dataset should load; run from the repo root"),
                strategy: Box::new(RSI::new(14)),
            })
            .collect()
    };

    let bars = load()[0].candles.len();
    println!("{} assets x {bars} bars\n", symbols.len());

    let cases: Vec<(&str, WeightPolicy)> = vec![
        ("manual         ", WeightPolicy::Manual),
        (
            "static minvar  ",
            WeightPolicy::Static {
                optimizer: OptimizerConfig::new(Objective::MinVariance),
                warmup: 252,
            },
        ),
        (
            "dyn riskparity ",
            WeightPolicy::Dynamic {
                optimizer: OptimizerConfig::new(Objective::RiskParity),
                lookback: 252,
            },
        ),
        (
            "dyn minvar     ",
            WeightPolicy::Dynamic {
                optimizer: OptimizerConfig::new(Objective::MinVariance),
                lookback: 252,
            },
        ),
        (
            "dyn maxsharpe  ",
            WeightPolicy::Dynamic {
                optimizer: OptimizerConfig::new(Objective::MaxSharpe),
                lookback: 252,
            },
        ),
    ];

    for (label, policy) in cases {
        let started = Instant::now();
        let res = run_portfolio_with_policy(
            load(),
            100_000.0,
            ExecutionCosts::default(),
            Some(RebalanceConfig { frequency: RebalanceFrequency::Monthly }),
            FillTiming::NextOpen,
            policy,
        );
        println!(
            "{label} {:>8.1?}  solves={:<4} rebalances={:<4} return={:>8.2}%  vol={:>6.2}%  sharpe={:>5.2}  maxdd={:>7.2}%",
            started.elapsed(),
            res.weight_history.len(),
            res.rebalance_dates.len(),
            res.metrics.total_return * 100.0,
            res.metrics.annualized_volatility * 100.0,
            res.metrics.sharpe_ratio,
            res.metrics.max_drawdown * 100.0,
        );
    }

    // The whole-run timings above are dominated by CSV loading, so isolate the
    // thing being claimed: what one solve actually costs.
    println!("\nisolated solve cost (252-bar window, {} assets):", symbols.len());
    let returns: Vec<Vec<f64>> = load()
        .iter()
        .map(|a| {
            a.candles
                .windows(2)
                .map(|w| w[1].close / w[0].close - 1.0)
                .take(252)
                .collect()
        })
        .collect();

    for objective in [
        Objective::InverseVolatility,
        Objective::RiskParity,
        Objective::MinVariance,
        Objective::MaxSharpe,
    ] {
        let cfg = OptimizerConfig::new(objective);
        let reps = 10_000;
        let started = Instant::now();
        let mut sink = 0.0;
        for _ in 0..reps {
            sink += backtesting::optimize::optimize_weights(&returns, &cfg).weights[0];
        }
        let per = started.elapsed() / reps;
        println!("  {objective:<20?} {per:>9.1?} per solve   (checksum {sink:.3})");
    }
}
