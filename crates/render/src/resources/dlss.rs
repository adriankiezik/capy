use std::sync::{Arc, Mutex};

use bevy_ecs::resource::Resource;
use glam::{UVec2, Vec2};
use uuid::Uuid;

use crate::Result;
use crate::gpu_texture::GpuTexture;

/// Stable application identifier for the NVIDIA DLSS SDK.
const DEFAULT_PROJECT_ID: Uuid = Uuid::from_bytes([
    0xa3, 0x6e, 0x7f, 0x01, 0xd4, 0x8b, 0x4e, 0x3a, 0x9c, 0x12, 0xb7, 0x5f, 0x0e, 0x6d, 0x81, 0xc4,
]);

#[derive(Resource, Clone, Debug)]
pub struct DlssSettings {
    pub project_id: Uuid,
    pub enabled: bool,
    pub perf_quality: DlssQualityMode,
    pub reset: bool,
    /// Set by the render crate after probing hardware.
    pub supported: bool,
    /// Set by the render crate — true when the GPU/driver supports DLSS Ray Reconstruction.
    pub ray_reconstruction_supported: bool,
    /// User toggle for DLSS Frame Generation. Default: `false`.
    pub frame_generation_enabled: bool,
    /// Set by the render crate — true when the GPU/driver supports DLSS Frame Generation.
    pub frame_generation_supported: bool,
    /// User toggle for NVIDIA Reflex low-latency mode. Default: `true`.
    pub reflex_enabled: bool,
    /// Set by the render crate — true when the GPU/driver supports Reflex.
    pub reflex_supported: bool,
}

impl Default for DlssSettings {
    fn default() -> Self {
        Self {
            project_id: DEFAULT_PROJECT_ID,
            enabled: true,
            perf_quality: DlssQualityMode::Auto,
            reset: false,
            supported: false,
            ray_reconstruction_supported: false,
            frame_generation_enabled: false,
            frame_generation_supported: false,
            reflex_enabled: true,
            reflex_supported: false,
        }
    }
}

impl DlssSettings {
    pub fn new(project_id: Uuid) -> Self {
        Self {
            project_id,
            ..Self::default()
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum DlssQualityMode {
    #[default]
    Auto,
    Dlaa,
    Quality,
    Balanced,
    Performance,
    UltraPerformance,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DlssCapability {
    Unknown,
    Supported,
    Unsupported,
}

pub(crate) struct DlssPipeline {
    capability: DlssCapability,
    sdk: Option<Arc<Mutex<crate::dlss::DlssSdk>>>,
    context: Option<crate::dlss::super_resolution::DlssSuperResolution>,
    output_texture: Option<GpuTexture>,
    perf_quality: DlssQualityMode,
    output_size: [u32; 2],
    rr_context: Option<crate::dlss::ray_reconstruction::DlssRayReconstruction>,
    rr_output_texture: Option<GpuTexture>,
    black_texture: Option<GpuTexture>,
    white_texture: Option<GpuTexture>,
    /// DLSS Frame Generation context.
    fg_context: Option<crate::dlss::frame_generation::DlssFrameGeneration>,
    /// Texture that receives the interpolated frame from Frame Generation.
    fg_output: Option<GpuTexture>,
    /// Separate texture for FG OutputReal (must differ from backbuffer input).
    fg_real_output: Option<GpuTexture>,
    /// Incremented whenever output textures are recreated (quality change, SR↔RR switch).
    generation: u32,
}

impl DlssPipeline {
    pub(crate) fn new() -> Self {
        Self {
            capability: DlssCapability::Unknown,
            sdk: None,
            context: None,
            output_texture: None,
            perf_quality: DlssQualityMode::Dlaa,
            output_size: [0, 0],
            rr_context: None,
            rr_output_texture: None,
            black_texture: None,
            white_texture: None,
            fg_context: None,
            fg_output: None,
            fg_real_output: None,
            generation: 0,
        }
    }

    pub(crate) fn is_supported(&self) -> bool {
        self.capability == DlssCapability::Supported
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
            .map(|context| context.render_resolution().to_array())
    }

    pub(crate) fn suggested_jitter(&self, frame_index: u32) -> Option<[f32; 2]> {
        if let Some(context) = &self.context {
            return Some(
                context
                    .suggested_jitter(frame_index, context.render_resolution())
                    .to_array(),
            );
        }
        if let Some(rr_context) = &self.rr_context {
            return Some(
                rr_context
                    .suggested_jitter(frame_index, rr_context.render_resolution())
                    .to_array(),
            );
        }
        None
    }

    pub(crate) fn configure(
        &mut self,
        settings: &DlssSettings,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        adapter: &wgpu::Adapter,
        dlss_extensions_enabled: bool,
        output_size: [u32; 2],
    ) -> Option<([u32; 2], bool)> {
        if !settings.enabled {
            self.deactivate();
            return None;
        }

        if !dlss_extensions_enabled {
            if self.capability != DlssCapability::Unsupported {
                tracing::warn!(
                    "DLSS requested, but the GPU device was not initialized with DLSS Vulkan extensions. \
                     Insert DlssSettings before RenderPlugin startup to enable it."
                );
            }
            self.capability = DlssCapability::Unsupported;
            self.deactivate();
            return None;
        }

        if adapter.get_info().backend != wgpu::Backend::Vulkan {
            if self.capability != DlssCapability::Unsupported {
                tracing::warn!(
                    "DLSS requires a Vulkan adapter; falling back to the standard blit path."
                );
            }
            self.capability = DlssCapability::Unsupported;
            self.deactivate();
            return None;
        }

        if self.capability == DlssCapability::Unsupported && self.sdk.is_none() {
            self.deactivate();
            return None;
        }

        let sdk = match self.sdk.clone() {
            Some(sdk) => sdk,
            None => match crate::dlss::DlssSdk::new(settings.project_id, device.clone()) {
                Ok(sdk) => {
                    self.capability = DlssCapability::Supported;
                    self.sdk = Some(sdk.clone());
                    sdk
                }
                Err(error) => {
                    tracing::warn!(
                        "Failed to initialize DLSS SDK: {error}. Falling back to the standard blit path."
                    );
                    self.capability = DlssCapability::Unsupported;
                    self.deactivate();
                    return None;
                }
            },
        };

        let recreate = self.context.is_none()
            || self.perf_quality != settings.perf_quality
            || self.output_size != output_size;

        if recreate {
            let feature_flags = crate::dlss::DlssFeatureFlags::LowResolutionMotionVectors
                | crate::dlss::DlssFeatureFlags::AutoExposure;
            let context = match crate::dlss::super_resolution::DlssSuperResolution::new(
                UVec2::new(output_size[0], output_size[1]),
                settings.perf_quality.into(),
                feature_flags,
                sdk,
                device,
                queue,
            ) {
                Ok(context) => context,
                Err(error) => {
                    tracing::warn!(
                        "Failed to create DLSS super-resolution context: {error}. Falling back to the standard blit path."
                    );
                    self.capability = DlssCapability::Unsupported;
                    self.deactivate();
                    return None;
                }
            };

            self.output_texture = Some(GpuTexture::new_2d(
                device,
                "DLSS Output",
                output_size[0],
                output_size[1],
                wgpu::TextureFormat::Rgba8Unorm,
                wgpu::TextureUsages::STORAGE_BINDING
                    | wgpu::TextureUsages::TEXTURE_BINDING
                    | wgpu::TextureUsages::COPY_DST
                    | wgpu::TextureUsages::RENDER_ATTACHMENT,
            ));
            self.perf_quality = settings.perf_quality;
            self.output_size = output_size;
            self.generation = self.generation.wrapping_add(1);
            let render_resolution = context.render_resolution().to_array();
            self.context = Some(context);
            return Some((render_resolution, true));
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
    ) -> Result<Option<wgpu::CommandBuffer>> {
        let (Some(context), Some(output_texture)) = (&mut self.context, &self.output_texture)
        else {
            return Ok(None);
        };

        let render_resolution = context.render_resolution();
        let render_parameters =
            crate::dlss::super_resolution::DlssSuperResolutionRenderParameters {
                color,
                depth,
                motion_vectors,
                exposure: crate::dlss::super_resolution::DlssSuperResolutionExposure::Automatic,
                bias: None,
                dlss_output: &output_texture.view,
                reset,
                jitter_offset: Vec2::new(-jitter[0], -jitter[1]),
                partial_texture_size: Some(render_resolution),
                motion_vector_scale: Some(Vec2::new(
                    -(render_resolution.x as f32),
                    -(render_resolution.y as f32),
                )),
            };

        let command_buffer = context.render(render_parameters, command_encoder, adapter)?;
        Ok(Some(command_buffer))
    }

    pub(crate) fn rr_output_texture(&self) -> Option<&GpuTexture> {
        self.rr_output_texture.as_ref()
    }

    #[allow(dead_code)]
    pub(crate) fn configure_ray_reconstruction(
        &mut self,
        settings: &DlssSettings,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        output_size: [u32; 2],
    ) -> Option<([u32; 2], bool)> {
        let sdk = self.sdk.clone()?;

        let recreate = self.rr_context.is_none()
            || self.perf_quality != settings.perf_quality
            || self.output_size != output_size;

        if recreate {
            // DLSS Ray Reconstruction requires HDR mode
            let feature_flags = crate::dlss::DlssFeatureFlags::LowResolutionMotionVectors
                | crate::dlss::DlssFeatureFlags::AutoExposure
                | crate::dlss::DlssFeatureFlags::HighDynamicRange;

            let context = match crate::dlss::ray_reconstruction::DlssRayReconstruction::new(
                UVec2::new(output_size[0], output_size[1]),
                settings.perf_quality.into(),
                feature_flags,
                crate::dlss::ray_reconstruction::DlssRayReconstructionRoughnessMode::Unpacked,
                crate::dlss::ray_reconstruction::DlssRayReconstructionDepthMode::Hardware,
                sdk,
                device,
                queue,
            ) {
                Ok(context) => context,
                Err(error) => {
                    tracing::warn!(
                        "Failed to create DLSS Ray Reconstruction context: {error}. Falling back."
                    );
                    self.deactivate_ray_reconstruction();
                    return None;
                }
            };

            let render_res = context.render_resolution();

            // Only deactivate SR after RR creation succeeds
            self.context = None;
            self.output_texture = None;

            self.rr_output_texture = Some(GpuTexture::new_2d(
                device,
                "DLSS RR Output",
                output_size[0],
                output_size[1],
                wgpu::TextureFormat::Rgba16Float,
                wgpu::TextureUsages::STORAGE_BINDING
                    | wgpu::TextureUsages::TEXTURE_BINDING
                    | wgpu::TextureUsages::COPY_DST
                    | wgpu::TextureUsages::RENDER_ATTACHMENT,
            ));

            // Black texture for specular_albedo (diffuse-only engine)
            // Created at render resolution since RR inputs are at render res
            let black_tex = GpuTexture::new_2d(
                device,
                "DLSS RR Black",
                render_res.x,
                render_res.y,
                wgpu::TextureFormat::Rgba8Unorm,
                wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            );
            // Texture is zero-initialized by default (all black)
            self.black_texture = Some(black_tex);

            // White texture for roughness (fully diffuse = roughness 1.0)
            // Created at render resolution since RR inputs are at render res
            let white_tex = GpuTexture::new_2d(
                device,
                "DLSS RR White",
                render_res.x,
                render_res.y,
                wgpu::TextureFormat::R8Unorm,
                wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            );
            // Fill with 0xFF via queue.write_texture
            let white_data = vec![0xFFu8; (render_res.x * render_res.y) as usize];
            queue.write_texture(
                wgpu::TexelCopyTextureInfo {
                    texture: &white_tex.texture,
                    mip_level: 0,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::All,
                },
                &white_data,
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(render_res.x),
                    rows_per_image: None,
                },
                wgpu::Extent3d {
                    width: render_res.x,
                    height: render_res.y,
                    depth_or_array_layers: 1,
                },
            );
            self.white_texture = Some(white_tex);

            self.perf_quality = settings.perf_quality;
            self.output_size = output_size;
            self.generation = self.generation.wrapping_add(1);
            let render_resolution = context.render_resolution().to_array();
            self.rr_context = Some(context);
            return Some((render_resolution, true));
        }

        self.rr_context
            .as_ref()
            .map(|ctx| (ctx.render_resolution().to_array(), false))
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn render_ray_reconstruction(
        &mut self,
        command_encoder: &mut wgpu::CommandEncoder,
        adapter: &wgpu::Adapter,
        diffuse_albedo: &wgpu::TextureView,
        normals: &wgpu::TextureView,
        color: &wgpu::TextureView,
        depth: &wgpu::TextureView,
        motion_vectors: &wgpu::TextureView,
        reset: bool,
        jitter: [f32; 2],
    ) -> Result<Option<wgpu::CommandBuffer>> {
        let (Some(context), Some(output_texture), Some(black_tex), Some(white_tex)) = (
            &mut self.rr_context,
            &self.rr_output_texture,
            &self.black_texture,
            &self.white_texture,
        ) else {
            return Ok(None);
        };

        let render_resolution = context.render_resolution();
        let render_parameters =
            crate::dlss::ray_reconstruction::DlssRayReconstructionRenderParameters {
                diffuse_albedo,
                specular_albedo: &black_tex.view,
                normals,
                roughness: Some(&white_tex.view),
                color,
                depth,
                motion_vectors,
                specular_guide:
                    crate::dlss::ray_reconstruction::DlssRayReconstructionSpecularGuide::SpecularMotionVectors(
                        motion_vectors,
                    ),
                screen_space_subsurface_scattering_guide: None,
                bias: None,
                dlss_output: &output_texture.view,
                reset,
                jitter_offset: Vec2::new(-jitter[0], -jitter[1]),
                partial_texture_size: Some(render_resolution),
                motion_vector_scale: Some(Vec2::new(
                    -(render_resolution.x as f32),
                    -(render_resolution.y as f32),
                )),
            };

        let command_buffer = context.render(render_parameters, command_encoder, adapter)?;
        Ok(Some(command_buffer))
    }

    // --- Frame Generation -------------------------------------------------

    pub(crate) fn fg_output(&self) -> Option<&GpuTexture> {
        self.fg_output.as_ref()
    }

    /// Create or destroy the Frame Generation context based on current settings.
    ///
    /// Returns `true` when FG is active after this call.
    pub(crate) fn configure_frame_generation(
        &mut self,
        settings: &DlssSettings,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        adapter: &wgpu::Adapter,
        output_size: [u32; 2],
        reflex_active: bool,
    ) -> bool {
        // FG requires: SDK initialised, Reflex active, an upscaler producing frames,
        // user opted in, and hardware support.
        let upscaler_active = self.output_texture.is_some() || self.rr_output_texture.is_some();
        let should_enable = settings.frame_generation_enabled
            && settings.frame_generation_supported
            && reflex_active
            && upscaler_active
            && self.capability == DlssCapability::Supported;

        if !should_enable {
            self.deactivate_frame_generation();
            return false;
        }

        let sdk = match self.sdk.clone() {
            Some(sdk) => sdk,
            None => {
                self.deactivate_frame_generation();
                return false;
            }
        };

        let recreate = self.fg_context.is_none() || self.output_size != output_size;

        if recreate {
            let context = match crate::dlss::frame_generation::DlssFrameGeneration::new(
                UVec2::new(output_size[0], output_size[1]),
                sdk,
                device,
                queue,
                adapter,
            ) {
                Ok(ctx) => ctx,
                Err(error) => {
                    tracing::warn!(
                        "Failed to create DLSS Frame Generation context: {error}. Falling back."
                    );
                    self.deactivate_frame_generation();
                    return false;
                }
            };

            let fg_usages = wgpu::TextureUsages::STORAGE_BINDING
                | wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::COPY_DST
                | wgpu::TextureUsages::RENDER_ATTACHMENT;
            self.fg_output = Some(GpuTexture::new_2d(
                device,
                "DLSS FG Interpolated",
                output_size[0],
                output_size[1],
                wgpu::TextureFormat::Rgba8Unorm,
                fg_usages,
            ));
            self.fg_real_output = Some(GpuTexture::new_2d(
                device,
                "DLSS FG Real Output",
                output_size[0],
                output_size[1],
                wgpu::TextureFormat::Rgba8Unorm,
                fg_usages,
            ));
            self.fg_context = Some(context);
            self.generation = self.generation.wrapping_add(1);
        }

        true
    }

    /// Evaluate Frame Generation using the current upscaler output as the backbuffer.
    ///
    /// `fallback_backbuffer` is used when no DLSS upscaler output is available
    /// (e.g. the lighting output when only FSR is active — unlikely with FG,
    /// but safe as a fallback).
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn evaluate_frame_generation(
        &mut self,
        command_encoder: &mut wgpu::CommandEncoder,
        adapter: &wgpu::Adapter,
        fallback_backbuffer: &wgpu::TextureView,
        depth: &wgpu::TextureView,
        motion_vectors: &wgpu::TextureView,
        reset: bool,
        camera: crate::dlss::frame_generation::FgCameraParams,
    ) -> crate::Result<Option<wgpu::CommandBuffer>> {
        let Some(context) = &mut self.fg_context else {
            return Ok(None);
        };
        let (Some(fg_output), Some(fg_real)) = (&self.fg_output, &self.fg_real_output) else {
            return Ok(None);
        };

        // Select the best backbuffer: prefer RR output, then SR output, then fallback.
        let backbuffer = self
            .rr_output_texture
            .as_ref()
            .or(self.output_texture.as_ref())
            .map(|t| &t.view)
            .unwrap_or(fallback_backbuffer);

        let params = crate::dlss::frame_generation::DlssFrameGenerationEvalParams {
            backbuffer,
            hudless: backbuffer, // No HUD separation in the engine yet.
            depth,
            motion_vectors,
            output: &fg_output.view,
            real_output: &fg_real.view,
            reset,
            camera,
        };

        let command_buffer = context.evaluate(params, command_encoder, adapter)?;
        Ok(Some(command_buffer))
    }

    pub(crate) fn deactivate_frame_generation(&mut self) {
        let was_active = self.fg_context.is_some();
        self.fg_context = None;
        self.fg_output = None;
        self.fg_real_output = None;
        if was_active {
            self.generation = self.generation.wrapping_add(1);
        }
    }

    // --- Ray Reconstruction ------------------------------------------------

    pub(crate) fn deactivate_ray_reconstruction(&mut self) {
        let was_active = self.rr_context.is_some();
        self.rr_context = None;
        self.rr_output_texture = None;
        self.black_texture = None;
        self.white_texture = None;
        if was_active {
            self.generation = self.generation.wrapping_add(1);
        }
    }

    pub(crate) fn deactivate(&mut self) {
        let was_active =
            self.context.is_some() || self.rr_context.is_some() || self.fg_context.is_some();
        self.context = None;
        self.output_texture = None;
        self.output_size = [0, 0];
        self.rr_context = None;
        self.rr_output_texture = None;
        self.black_texture = None;
        self.white_texture = None;
        self.fg_context = None;
        self.fg_output = None;
        self.fg_real_output = None;
        if was_active {
            self.generation = self.generation.wrapping_add(1);
        }
    }
}

impl From<DlssQualityMode> for crate::dlss::DlssPerfQualityMode {
    fn from(value: DlssQualityMode) -> Self {
        match value {
            DlssQualityMode::Auto => Self::Auto,
            DlssQualityMode::Dlaa => Self::Dlaa,
            DlssQualityMode::Quality => Self::Quality,
            DlssQualityMode::Balanced => Self::Balanced,
            DlssQualityMode::Performance => Self::Performance,
            DlssQualityMode::UltraPerformance => Self::UltraPerformance,
        }
    }
}
