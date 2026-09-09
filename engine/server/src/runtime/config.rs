#[derive(Clone, Copy, Debug)]
pub struct RuntimeConfig {
    pub max_catch_up_ticks: u32,
}

impl Default for RuntimeConfig {
    fn default() -> Self {
        Self {
            max_catch_up_ticks: 4,
        }
    }
}
