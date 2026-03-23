use bevy_ecs::resource::Resource;
use glam::UVec2;

use crate::Result;
use crate::gpu_texture::GpuTexture;

#[derive(Resource, Clone, Debug)]
pub struct FsrSettings {
    pub enabled: bool,
    pub quality: FsrQualityMode,
    pub reset: bool,
    /// Set by the render crate after probing hardware.
    pub supported: bool,
}

impl Default for FsrSettings {
    fn default() -> Self {
        Self {
            // When DLSS is also available, prefer DLSS by default.
            enabled: true,
            quality: FsrQualityMode::Auto,
            reset: false,
            supported: false,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum FsrQualityMode {
    #[default]
    Auto,
    NativeAA,
    Quality,
    Balanced,
    Performance,
    UltraPerformance,
}

impl FsrQualityMode {
    /// Returns the upscale ratio for this quality mode.
    fn scale_factor(self) -> f32 {
        match self {
            Self::Auto => 1.0, // resolved elsewhere
            Self::NativeAA => 1.0,
            Self::Quality => 1.5,
            Self::Balanced => 1.7,
            Self::Performance => 2.0,
            Self::UltraPerformance => 3.0,
        }
    }

    /// Select a quality mode automatically based on output resolution megapixels.
    fn auto_select(output_size: UVec2) -> Self {
        let mega_pixels = (output_size.x * output_size.y) as f32 / 1_000_000.0;
        if mega_pixels < 2.03 {
            Self::NativeAA
        } else if mega_pixels < 3.68 {
            Self::Quality
        } else if mega_pixels < 8.29 {
            Self::Performance
        } else {
            Self::UltraPerformance
        }
    }

    /// Compute the render resolution for the given output resolution.
    pub(crate) fn render_resolution(self, output_size: UVec2) -> UVec2 {
        let resolved = if self == Self::Auto {
            Self::auto_select(output_size)
        } else {
            self
        };
        let scale = resolved.scale_factor();
        UVec2::new(
            ((output_size.x as f32) / scale).round().max(1.0) as u32,
            ((output_size.y as f32) / scale).round().max(1.0) as u32,
        )
    }
}

pub(crate) struct FsrPipeline {
    context: Option<crate::fsr::FsrContext>,
    output_texture: Option<GpuTexture>,
    quality: FsrQualityMode,
    output_size: [u32; 2],
    /// Incremented whenever the output texture is recreated.
    generation: u32,
}

impl FsrPipeline {
    pub(crate) fn new() -> Self {
        Self {
            context: None,
            output_texture: None,
            quality: FsrQualityMode::NativeAA,
            output_size: [0, 0],
            generation: 0,
        }
    }

    pub(crate) fn generation(&self) -> u32 {
        self.generation
    }

    pub(crate) fn output_texture(&self) -> Option<&GpuTexture> {
        self.output_texture.as_ref()
    }

    pub(crate) fn render_resolution(&self) -> Option<[u32; 2]> {
        self.context
            .as_ref()
            .map(|ctx| ctx.render_resolution().to_array())
    }

    pub(crate) fn suggested_jitter(&self, frame_index: u32) -> Option<[f32; 2]> {
        self.context
            .as_ref()
            .map(|ctx| ctx.suggested_jitter(frame_index).to_array())
    }

    pub(crate) fn configure(
        &mut self,
        settings: &FsrSettings,
        device: &wgpu::Device,
        adapter: &wgpu::Adapter,
        output_size: [u32; 2],
    ) -> Option<([u32; 2], bool)> {
        if !settings.enabled {
            self.deactivate();
            return None;
        }

        if adapter.get_info().backend != wgpu::Backend::Vulkan {
            tracing::warn!(
                "FSR requires a Vulkan adapter; falling back to the standard blit path."
            );
            self.deactivate();
            return None;
        }

        let recreate = self.context.is_none()
            || self.quality != settings.quality
            || self.output_size != output_size;

        if recreate {
            let output_uvec = UVec2::new(output_size[0], output_size[1]);
            let render_res = settings.quality.render_resolution(output_uvec);

            match crate::fsr::FsrContext::new(device, adapter, output_uvec, render_res, true) {
                Ok(context) => {
                    self.output_texture = Some(GpuTexture::new_2d(
                        device,
                        "FSR Output",
                        output_size[0],
                        output_size[1],
                        wgpu::TextureFormat::Rgba8Unorm,
                        wgpu::TextureUsages::STORAGE_BINDING
                            | wgpu::TextureUsages::TEXTURE_BINDING
                            | wgpu::TextureUsages::COPY_DST
                            | wgpu::TextureUsages::RENDER_ATTACHMENT,
                    ));
                    self.quality = settings.quality;
                    self.output_size = output_size;
                    self.generation = self.generation.wrapping_add(1);
                    let render_resolution = context.render_resolution().to_array();
                    self.context = Some(context);
                    return Some((render_resolution, true));
                }
                Err(error) => {
                    tracing::warn!(
                        "Failed to create FSR context: {error}. Falling back to the standard blit path."
                    );
                    self.deactivate();
                    return None;
                }
            }
        }

        self.render_resolution()
            .map(|resolution| (resolution, false))
    }

    pub(crate) fn render(
        &mut self,
        command_encoder: &mut wgpu::CommandEncoder,
        adapter: &wgpu::Adapter,
        color: &wgpu::TextureView,
        depth: &wgpu::TextureView,
        motion_vectors: &wgpu::TextureView,
        reset: bool,
        jitter: [f32; 2],
        delta_time_ms: f32,
    ) -> Result<Option<wgpu::CommandBuffer>> {
        let (Some(context), Some(output_texture)) = (&mut self.context, &self.output_texture)
        else {
            return Ok(None);
        };

        let render_resolution = context.render_resolution();
        let cmd_buf = context.render(
            command_encoder,
            adapter,
            color,
            depth,
            motion_vectors,
            &output_texture.view,
            reset,
            glam::Vec2::new(-jitter[0], -jitter[1]),
            render_resolution,
            delta_time_ms,
        )?;

        Ok(Some(cmd_buf))
    }

    pub(crate) fn deactivate(&mut self) {
        let was_active = self.context.is_some();
        self.context = None;
        self.output_texture = None;
        self.output_size = [0, 0];
        if was_active {
            self.generation = self.generation.wrapping_add(1);
        }
    }
}
