use bevy_ecs::error::BevyError;
use bevy_ecs::system::NonSendMut;

use crate::resources::trace::TracePipeline;
use crate::resources::{BlitPipeline, FrameInProgress, GpuContext, LightingPipeline};

pub(crate) fn render_passes_system(
    gpu: NonSendMut<GpuContext>,
    trace: Option<NonSendMut<TracePipeline>>,
    lighting: Option<NonSendMut<LightingPipeline>>,
    blit: Option<NonSendMut<BlitPipeline>>,
    mut frame: NonSendMut<FrameInProgress>,
) -> Result<(), BevyError> {
    let (Some(trace), Some(lighting), Some(blit)) = (trace, lighting, blit) else {
        return Ok(());
    };

    let output = match gpu.surface.get_current_texture() {
        Ok(output) => output,
        Err(wgpu::SurfaceError::Lost | wgpu::SurfaceError::Outdated) => {
            gpu.surface.configure(&gpu.device, &gpu.config);
            gpu.surface.get_current_texture()?
        }
        Err(wgpu::SurfaceError::Timeout | wgpu::SurfaceError::Other) => return Ok(()),
        Err(error) => return Err(error.into()),
    };

    let output_view = output
        .texture
        .create_view(&wgpu::TextureViewDescriptor::default());

    let mut encoder = frame.encoder.take().unwrap_or_else(|| {
        gpu.device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Frame Encoder"),
            })
    });

    {
        let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: Some("Trace Pass"),
            ..Default::default()
        });
        pass.set_pipeline(&trace.compute_pipeline);
        pass.set_bind_group(0, &trace.compute_bind_group, &[]);
        pass.dispatch_workgroups(trace.width.div_ceil(8), trace.height.div_ceil(8), 1);
    }

    {
        let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: Some("Lighting Pass"),
            ..Default::default()
        });
        pass.set_pipeline(&lighting.compute_pipeline);
        pass.set_bind_group(0, &lighting.bind_group, &[]);
        pass.dispatch_workgroups(lighting.width.div_ceil(8), lighting.height.div_ceil(8), 1);
    }

    {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("Blit Pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &output_view,
                resolve_target: None,
                depth_slice: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            ..Default::default()
        });
        pass.set_pipeline(&blit.blit_pipeline);
        pass.set_bind_group(0, &blit.blit_bind_group, &[]);
        pass.draw(0..3, 0..1);
    }

    frame.encoder = Some(encoder);
    frame.output = Some(output);
    frame.output_view = Some(output_view);

    Ok(())
}
