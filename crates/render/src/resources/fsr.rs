use bevy_ecs::resource::Resource;
use glam::UVec2;

use crate::Result;
use crate::fsr::FsrFgCameraParams;
use crate::gpu_texture::GpuTexture;

#[derive(Resource, Clone, Debug)]
pub struct FsrSettings {
    pub enabled: bool,
    pub quality: FsrQualityMode,
    pub reset: bool,
    /// Set by the render crate after probing hardware.
    pub supported: bool,
    /// Reserved for future FSR 3 frame generation.
    pub frame_generation_enabled: bool,
    /// Set by the render crate after probing hardware.
    pub frame_generation_supported: bool,
}

impl Default for FsrSettings {
    fn default() -> Self {
        Self {
            // When DLSS is also available, prefer DLSS by default.
            enabled: true,
            quality: FsrQualityMode::Auto,
            reset: false,
            supported: false,
            frame_generation_enabled: false,
            frame_generation_supported: false,
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
    /// FSR 3 Frame Generation context.
    fg_context: Option<crate::fsr::FsrFrameGeneration>,
    /// Texture that receives the interpolated frame from Frame Generation.
    fg_output: Option<GpuTexture>,
}

impl FsrPipeline {
    pub(crate) fn new() -> Self {
        Self {
            context: None,
            output_texture: None,
            quality: FsrQualityMode::NativeAA,
            output_size: [0, 0],
            generation: 0,
            fg_context: None,
            fg_output: None,
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

    pub(crate) fn suggested_jitter(&mut self, frame_index: u32) -> Option<[f32; 2]> {
        self.context
            .as_mut()
            .map(|ctx| ctx.suggested_jitter(frame_index).to_array())
    }

    pub(crate) fn configure(
        &mut self,
        settings: &FsrSettings,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        adapter: &wgpu::Adapter,
        output_size: [u32; 2],
    ) -> Option<([u32; 2], bool)> {
        if !settings.enabled {
            self.deactivate();
            return None;
        }

        if adapter.get_info().backend != wgpu::Backend::Dx12 {
            tracing::warn!(
                "FSR 3.1 requires a DX12 adapter; falling back to the standard blit path."
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
            tracing::info!("FSR: creating context — output={output_uvec}, render={render_res}");

            match crate::fsr::FsrContext::new(device, queue, adapter, output_uvec, render_res, true)
            {
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
                    // Invalidate the FG context so it gets recreated at the
                    // new resolution in configure_frame_generation().
                    self.deactivate_frame_generation();
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
        encoder: &mut wgpu::CommandEncoder,
        queue: &wgpu::Queue,
        color: &wgpu::TextureView,
        depth: &wgpu::TextureView,
        motion_vectors: &wgpu::TextureView,
        reset: bool,
        jitter: [f32; 2],
        delta_time_ms: f32,
    ) -> Result<bool> {
        let (Some(context), Some(output_texture)) = (&mut self.context, &self.output_texture)
        else {
            return Ok(false);
        };

        let render_resolution = context.render_resolution();
        context.render(
            encoder,
            queue,
            color,
            depth,
            motion_vectors,
            &output_texture.view,
            reset,
            glam::Vec2::new(-jitter[0], -jitter[1]),
            render_resolution,
            delta_time_ms,
        )?;

        Ok(true)
    }

    // --- Frame Generation ---------------------------------------------------

    pub(crate) fn fg_output(&self) -> Option<&GpuTexture> {
        self.fg_output.as_ref()
    }

    /// Create or destroy the Frame Generation context based on current settings.
    ///
    /// Returns `true` when FG is active after this call.
    pub(crate) fn configure_frame_generation(
        &mut self,
        settings: &FsrSettings,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        adapter: &wgpu::Adapter,
        output_size: [u32; 2],
    ) -> bool {
        // FG requires: the upscaler producing frames, user opted in, and hw support.
        let upscaler_active = self.context.is_some();
        let should_enable = settings.frame_generation_enabled
            && settings.frame_generation_supported
            && upscaler_active;

        if !should_enable {
            self.deactivate_frame_generation();
            return false;
        }

        let recreate = self.fg_context.is_none() || self.output_size != output_size;

        if recreate {
            let render_res = self
                .render_resolution()
                .map(|r| UVec2::new(r[0], r[1]))
                .unwrap_or(UVec2::ONE);
            let display_size = UVec2::new(output_size[0], output_size[1]);
            tracing::info!(
                "FSR FG: creating context — display={display_size}, render={render_res}"
            );

            match crate::fsr::FsrFrameGeneration::new(
                device,
                queue,
                adapter,
                display_size,
                render_res,
            ) {
                Ok(fg) => {
                    let fg_usages = wgpu::TextureUsages::STORAGE_BINDING
                        | wgpu::TextureUsages::TEXTURE_BINDING
                        | wgpu::TextureUsages::COPY_DST
                        | wgpu::TextureUsages::RENDER_ATTACHMENT;
                    self.fg_output = Some(GpuTexture::new_2d(
                        device,
                        "FSR FG Interpolated",
                        output_size[0],
                        output_size[1],
                        wgpu::TextureFormat::Rgba8Unorm,
                        fg_usages,
                    ));
                    self.fg_context = Some(fg);
                    self.generation = self.generation.wrapping_add(1);
                }
                Err(error) => {
                    tracing::warn!("Failed to create FSR Frame Generation context: {error}");
                    self.deactivate_frame_generation();
                    return false;
                }
            }
        }

        true
    }

    /// Evaluate Frame Generation using the current upscaler output as input.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn evaluate_frame_generation(
        &mut self,
        encoder: &mut wgpu::CommandEncoder,
        queue: &wgpu::Queue,
        depth: &wgpu::TextureView,
        motion_vectors: &wgpu::TextureView,
        reset: bool,
        jitter: [f32; 2],
        delta_time_ms: f32,
        camera: FsrFgCameraParams,
    ) -> Result<bool> {
        let (Some(fg), Some(upscaler_output), Some(fg_output)) =
            (&mut self.fg_context, &self.output_texture, &self.fg_output)
        else {
            return Ok(false);
        };

        let render_size = self
            .context
            .as_ref()
            .map(|ctx| ctx.render_resolution())
            .unwrap_or(UVec2::ONE);

        let produced = fg.evaluate(
            encoder,
            queue,
            &upscaler_output.view,
            depth,
            motion_vectors,
            &fg_output.view,
            reset,
            glam::Vec2::new(-jitter[0], -jitter[1]),
            render_size,
            delta_time_ms,
            &camera,
        )?;

        Ok(produced)
    }

    pub(crate) fn deactivate_frame_generation(&mut self) {
        let was_active = self.fg_context.is_some();
        self.fg_context = None;
        self.fg_output = None;
        if was_active {
            self.generation = self.generation.wrapping_add(1);
        }
    }

    // --- Deactivation -------------------------------------------------------

    pub(crate) fn deactivate(&mut self) {
        let was_active = self.context.is_some() || self.fg_context.is_some();
        self.context = None;
        self.output_texture = None;
        self.output_size = [0, 0];
        self.fg_context = None;
        self.fg_output = None;
        if was_active {
            self.generation = self.generation.wrapping_add(1);
        }
    }
}
