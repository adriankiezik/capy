use bevy_ecs::resource::Resource;
use glam::Vec3;

/// How the path tool modifies the terrain.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PathMode {
    /// Only repaint the surface voxels along the path.
    Paint,
    /// Flatten terrain to the interpolated path height, then paint.
    Flatten,
}

/// Persistent state for the smart path creation tool.
#[derive(Resource)]
pub struct PathState {
    /// World-space waypoints placed by the user.
    pub waypoints: Vec<Vec3>,
    /// Half-width of the path in voxels.
    pub path_width: u32,
    /// Whether to paint-only or flatten+paint.
    pub mode: PathMode,
    /// When true, user has confirmed the path — apply edits and clear.
    pub confirmed: bool,
    /// When true, the waypoint list changed and the preview needs recomputing.
    pub dirty: bool,
}

impl Default for PathState {
    fn default() -> Self {
        Self {
            waypoints: Vec::new(),
            path_width: 3,
            mode: PathMode::Flatten,
            confirmed: false,
            dirty: false,
        }
    }
}
