#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FrameIndex {
    pub absolute_frame: u32,
    pub measured_frame: Option<u32>,
}

#[derive(serde::Serialize)]
pub struct MeasurementFrame {
    pub absolute_frame: u32,
    pub measured_frame: u32,
    pub update_ms: f64,
    pub prepare_ms: f64,
    pub encode_ms: f64,
    pub wall_ms: f64,
    pub gpu_ms: Option<f64>,
}

#[derive(serde::Serialize)]
pub struct MeasurementReport {
    pub adapter: String,
    pub backend: String,
    pub driver: String,
    pub width: u32,
    pub height: u32,
    pub warmup: u32,
    pub capture_hash: String,
    pub depth_hash: String,
    pub geometry_bytes: usize,
    pub mesh_instance_count: usize,
    pub samples: Vec<MeasurementFrame>,
}
