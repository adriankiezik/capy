use bevy_ecs::resource::Resource;

#[derive(Resource, Debug)]
pub(crate) struct TemporalCameraState {
    frame_index: u32,
    current_jitter: [f32; 2],
    previous_clip_from_world: [f32; 16],
    pending_clip_from_world: [f32; 16],
    has_previous: bool,
    has_pending: bool,
}

impl Default for TemporalCameraState {
    fn default() -> Self {
        Self {
            frame_index: 0,
            current_jitter: [0.0, 0.0],
            previous_clip_from_world: [0.0; 16],
            pending_clip_from_world: [0.0; 16],
            has_previous: false,
            has_pending: false,
        }
    }
}

impl TemporalCameraState {
    #[cfg(feature = "dlss")]
    pub(crate) fn frame_index(&self) -> u32 {
        self.frame_index
    }

    #[cfg(feature = "dlss")]
    pub(crate) fn current_jitter(&self) -> [f32; 2] {
        self.current_jitter
    }

    pub(crate) fn previous_clip_from_world(
        &self,
        fallback_clip_from_world: [f32; 16],
    ) -> [f32; 16] {
        if self.has_previous {
            self.previous_clip_from_world
        } else {
            fallback_clip_from_world
        }
    }

    pub(crate) fn set_current_frame(&mut self, clip_from_world: [f32; 16], jitter: [f32; 2]) {
        self.pending_clip_from_world = clip_from_world;
        self.current_jitter = jitter;
        self.has_pending = true;
    }

    pub(crate) fn finish_frame(&mut self) {
        if self.has_pending {
            self.previous_clip_from_world = self.pending_clip_from_world;
            self.has_previous = true;
            self.has_pending = false;
            self.frame_index = self.frame_index.wrapping_add(1);
        }
    }

    pub(crate) fn reset_history(&mut self) {
        self.has_previous = false;
    }
}
