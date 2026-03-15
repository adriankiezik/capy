use bevy_ecs::error::BevyError;
use bevy_ecs::system::NonSendMut;

use crate::resources::{BlitPipeline, FrameData, FrameInProgress, GpuContext, StreamingPipeline};

pub(crate) fn render_passes_system(
    gpu: NonSendMut<GpuContext>,
    streaming: Option<NonSendMut<StreamingPipeline>>,
    blit: Option<NonSendMut<BlitPipeline>>,
    mut frame: NonSendMut<FrameInProgress>,
) -> Result<(), BevyError> {
    let (Some(streaming), Some(blit)) = (streaming, blit) else {
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

    let mut encoder = gpu
        .device
        .create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("Render Encoder"),
        });

    {
        let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: Some("Trace Pass"),
            ..Default::default()
        });
        pass.set_pipeline(&streaming.compute_pipeline);
        pass.set_bind_group(0, &streaming.compute_bind_group, &[]);
        pass.dispatch_workgroups(
            output.texture.width().div_ceil(8),
            output.texture.height().div_ceil(8),
            1,
        );
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

    frame.data = Some(FrameData {
        encoder,
        output,
        output_view,
        device: gpu.device.clone(),
        queue: gpu.queue.clone(),
        surface_format: gpu.config.format,
    });

    Ok(())
}
