use chrono::NaiveDate;
use serde::{Deserialize, Serialize};
use std::error::Error;

/// An OHLCV bar representing one trading period.
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Candle {
    /// The date of this trading bar.
    #[serde(rename = "Date")]
    pub date: NaiveDate,

    /// Opening price of the period.
    #[serde(rename = "Open")]
    pub open: f64,

    /// Highest price reached during the period.
    #[serde(rename = "High")]
    pub high: f64,

    /// Lowest price reached during the period.
    #[serde(rename = "Low")]
    pub low: f64,

    /// Closing price of the period.
    #[serde(rename = "Close")]
    pub close: f64,

    /// Total shares traded during the period.
    #[serde(rename = "Volume")]
    pub volume: u64,
}

/// Loads candles from a CSV file with headers: `Date,Open,High,Low,Close,Volume`.
///
/// # Examples
///
/// ```no_run
/// use backtesting::data::{CSVDataSource, DataSource};
/// let candles = CSVDataSource { file_path: "prices.csv".to_string() }
///     .load()
///     .expect("failed to load");
/// println!("loaded {} bars", candles.len());
/// ```
pub struct CSVDataSource {
    pub file_path: String,
}

/// Trait for loading a sequence of candles from an external source.
pub trait DataSource {
    /// Load all candles in chronological order.
    ///
    /// Returns an error if the source is inaccessible or contains malformed data.
    fn load(&self) -> Result<Vec<Candle>, Box<dyn Error>>;
}

impl DataSource for CSVDataSource {
    fn load(&self) -> Result<Vec<Candle>, Box<dyn Error>> {
        let file = std::fs::File::open(&self.file_path)?;
        let mut rdr = csv::Reader::from_reader(file);
        let mut candles = Vec::new();
        for result in rdr.deserialize() {
            let candle: Candle = result?;
            candles.push(candle);
        }
        Ok(candles)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

    fn fixture(name: &str) -> String {
        format!("{}/../data/fixtures/{}", env!("CARGO_MANIFEST_DIR"), name)
    }

    #[test]
    fn csv_loads_correct_row_count() {
        let src = CSVDataSource { file_path: fixture("synthetic_30.csv") };
        let candles = src.load().expect("fixture should load");
        assert_eq!(candles.len(), 30);
    }

    #[test]
    fn csv_parses_first_candle_fields() {
        let src = CSVDataSource { file_path: fixture("synthetic_30.csv") };
        let candles = src.load().expect("fixture should load");
        let c = &candles[0];
        assert_eq!(c.date, NaiveDate::from_ymd_opt(2024, 1, 2).unwrap());
        assert!((c.open - 100.0).abs() < 1e-9);
        assert!((c.close - 100.0).abs() < 1e-9);
        assert_eq!(c.volume, 10000);
    }

    #[test]
    fn csv_parses_last_candle_date() {
        let src = CSVDataSource { file_path: fixture("synthetic_30.csv") };
        let candles = src.load().expect("fixture should load");
        assert_eq!(candles[29].date, NaiveDate::from_ymd_opt(2024, 1, 31).unwrap());
    }

    #[test]
    fn candle_high_gte_low_for_all_rows() {
        let src = CSVDataSource { file_path: fixture("synthetic_30.csv") };
        let candles = src.load().expect("fixture should load");
        for c in &candles {
            assert!(c.high >= c.low, "high < low on {}", c.date);
        }
    }

    #[test]
    fn csv_returns_error_on_missing_file() {
        let src = CSVDataSource { file_path: "/nonexistent/path.csv".to_string() };
        assert!(src.load().is_err());
    }

    #[test]
    fn csv_returns_error_on_malformed_row() {
        let src = CSVDataSource { file_path: fixture("malformed.csv") };
        assert!(src.load().is_err());
    }
}
