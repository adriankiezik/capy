#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PowerPreference {
    Default,
    LowPower,
    #[default]
    HighPerformance,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PresentationMode {
    Vsync,
    #[default]
    Immediate,
}

#[derive(Clone, Debug)]
pub struct GraphicsSettings {
    pub power_preference: PowerPreference,
    pub presentation: PresentationMode,
    pub maximum_frame_latency: u32,
}

impl Default for GraphicsSettings {
    fn default() -> Self {
        Self {
            power_preference: PowerPreference::default(),
            presentation: PresentationMode::default(),
            maximum_frame_latency: 2,
        }
    }
}
