use serde::Serialize;
use std::collections::HashMap;
use crate::metrics::Metrics;

/// One point in a parameter sweep: the parameter values tried and the resulting metrics.
#[derive(Serialize)]
pub struct SweepResult {
    pub params: HashMap<String, f64>,
    pub metrics: Metrics,
}
