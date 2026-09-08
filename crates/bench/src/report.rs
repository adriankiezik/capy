use crate::scenario::Kind;
use serde::Serialize;
use std::time::Duration;

#[derive(Serialize)]
pub struct Distribution {
    pub mean: f64,
    pub median: f64,
    pub p95: f64,
    pub p99: f64,
}

impl Distribution {
    pub fn milliseconds(values: &[Duration]) -> Option<Self> {
        if values.is_empty() {
            return None;
        }

        let mut values: Vec<f64> = values
            .iter()
            .map(|value| value.as_secs_f64() * 1000.0)
            .collect();

        values.sort_by(f64::total_cmp);

        let percentile = |fraction: f64| {
            let index = fraction * (values.len() - 1) as f64;

            let low = index.floor() as usize;

            let high = index.ceil() as usize;

            values[low] + (values[high] - values[low]) * index.fract()
        };

        Some(Self {
            mean: values.iter().sum::<f64>() / values.len() as f64,
            median: percentile(0.5),
            p95: percentile(0.95),
            p99: percentile(0.99),
        })
    }
}

#[derive(Serialize)]
pub struct Run {
    pub scenario: Kind,
    pub run: u32,
    pub frames: u32,
    pub setup_ms: f64,
    pub elapsed_seconds: f64,
    pub completed_frames_per_second: f64,
    pub cpu_frame_ms: Distribution,
    pub update_and_ui_ms: Distribution,
    pub render_prepare_ms: Distribution,
    pub encode_and_submit_ms: Distribution,
    pub backpressure_ms: Distribution,
    pub gpu_render_ms: Option<Distribution>,
}

#[derive(Serialize)]
pub struct Report {
    pub schema_version: u32,
    pub workload_version: u32,
    pub version: &'static str,
    pub mode: &'static str,
    pub os: &'static str,
    pub architecture: &'static str,
    pub debug_build: bool,
    pub unix_timestamp: u64,
    pub resolution: [u32; 2],
    pub simulation_step_ns: u128,
    pub warmup_frames: u32,
    pub frames_in_flight: usize,
    pub adapter: String,
    pub backend: String,
    pub driver: String,
    pub driver_info: String,
    pub device_type: String,
    pub gpu_timestamps: bool,
    pub runs: Vec<Run>,
}
