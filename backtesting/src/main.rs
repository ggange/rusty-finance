use backtesting::data::{CSVDataSource, DataSource};
use backtesting::engine::BacktestEngine;
use backtesting::portfolio::Portfolio;
use backtesting::strategy::ma::{MAType, MovingAverageCrossover};
use backtesting::strategy::rsi::RSI;

fn main() {
    let csv_path = std::env::args().nth(1).unwrap_or_else(|| {
        eprintln!("Usage: backtesting <path/to/prices.csv>");
        eprintln!("Example: backtesting data/fixtures/synthetic_30.csv");
        std::process::exit(1);
    });

    let candles = CSVDataSource { file_path: csv_path }
        .load()
        .expect("Failed to load CSV");

    // MA EMA(5, 20) strategy
    let mut engine_ma = BacktestEngine::new(
        MovingAverageCrossover::new(MAType::EMA, 5, 20),
        Portfolio::new(10_000.0, "SYMBOL".to_string()),
    );
    engine_ma.run(&candles);
    let result_ma = engine_ma.result();

    // RSI(14) strategy
    let mut engine_rsi = BacktestEngine::new(
        RSI::new(14),
        Portfolio::new(10_000.0, "SYMBOL".to_string()),
    );
    engine_rsi.run(&candles);
    let result_rsi = engine_rsi.result();

    let output = serde_json::json!({
        "ma_ema_5_20": result_ma,
        "rsi_14": result_rsi,
    });
    println!("{}", serde_json::to_string_pretty(&output).unwrap());
}
