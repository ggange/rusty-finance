use chrono::NaiveDate;
use serde::{Deserialize, Serialize};
use std::error::Error;

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Candle {
    #[serde(rename = "Date")]
    pub date: NaiveDate,

    #[serde(rename = "Open")]
    pub open: f64,

    #[serde(rename = "High")]
    pub high: f64,

    #[serde(rename = "Low")]
    pub low: f64,

    #[serde(rename = "Close")]
    pub close: f64,

    #[serde(rename = "Volume")]
    pub volume: u64,
}

pub struct CSVDataSource {
    pub file_path: String,
}

pub trait DataSource {
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
