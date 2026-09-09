#[derive(Clone, Copy, Debug)]
pub struct ModelConfig {
    pub bake_ambient_occlusion: bool,
    pub leaf_edge: i32,
    pub max_vertices: usize,
}

impl Default for ModelConfig {
    fn default() -> Self {
        Self {
            bake_ambient_occlusion: true,
            leaf_edge: 32,
            max_vertices: 1_000_000,
        }
    }
}
