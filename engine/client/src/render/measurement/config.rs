#[derive(Clone, Debug)]
pub struct MeasurementConfig {
    pub width: u32,
    pub height: u32,
    pub warmup: u32,
    pub frames: u32,
    pub capture: Option<CapturePaths>,
    pub gpu_timestamps: bool,
}

impl Default for MeasurementConfig {
    fn default() -> Self {
        Self {
            width: 2940,
            height: 1846,
            warmup: 200,
            frames: 600,
            capture: None,
            gpu_timestamps: true,
        }
    }
}

#[derive(Clone, Debug)]
pub struct CapturePaths {
    pub rgba8_srgb: std::path::PathBuf,
    pub depth_f32_le: std::path::PathBuf,
    pub camera_columns_eye_f32_le: std::path::PathBuf,
}
