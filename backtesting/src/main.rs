use backtesting::data::{CSVDataSource, DataSource};
use backtesting::strategy::ma::{MAType, MovingAverageCrossover};
use backtesting::strategy::rsi::RSI;
use backtesting::engine::BacktestEngine;
use backtesting::portfolio::Portfolio;

fn main() {
    let filename = "../stock-market-dataset/stocks/AAPL.csv".to_string();
    let data_source = CSVDataSource { file_path: filename };
    let candles = data_source.load().expect("Failed to load data");

    // MA EMA(5, 20) strategy
    let mut engine_ma = BacktestEngine::new(
        MovingAverageCrossover::new(MAType::EMA, 5, 20),
        Portfolio::new(10_000.0, "AAPL".to_string()),
    );
    engine_ma.run(&candles);
    let result_ma = engine_ma.result();

    // RSI(14) strategy
    let mut engine_rsi = BacktestEngine::new(
        RSI::new(14),
        Portfolio::new(10_000.0, "AAPL".to_string()),
    );
    engine_rsi.run(&candles);
    let result_rsi = engine_rsi.result();

    let output = serde_json::json!({
        "ma_ema_5_20": result_ma,
        "rsi_14": result_rsi,
    });
    println!("{}", serde_json::to_string_pretty(&output).unwrap());
}
