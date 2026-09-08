use crate::graphics::{FrameOutcome, Graphics, GraphicsError, Result};

#[derive(Debug)]
pub struct Frame {
    view: wgpu::TextureView,
    encoder: wgpu::CommandEncoder,
}

impl Frame {
    pub fn parts(&mut self) -> (&wgpu::TextureView, &mut wgpu::CommandEncoder) {
        (&self.view, &mut self.encoder)
    }
}

impl Graphics {
    pub(crate) fn render(
        &mut self,
        draw: impl FnOnce(&mut Frame) -> anyhow::Result<()>,
        before_present: impl FnOnce(),
    ) -> Result<FrameOutcome> {
        self.check_errors()?;

        if !self.drawable() {
            return Ok(FrameOutcome::Retry);
        }

        let p = self
            .presentation
            .as_ref()
            .ok_or(GraphicsError::NotPresenting)?;

        use wgpu::CurrentSurfaceTexture as Texture;

        let (texture, reconfigure) = match p.surface.get_current_texture() {
            Texture::Success(texture) => (texture, false),
            Texture::Suboptimal(texture) => (texture, true),
            Texture::Timeout | Texture::Occluded => return Ok(FrameOutcome::Retry),
            Texture::Outdated => {
                let configuration = p.config.clone();

                self.configure_presentation(configuration)?;

                return Ok(FrameOutcome::Retry);
            }
            Texture::Lost => {
                let (target, width, height) = (p.target.clone(), p.width, p.height);

                self.attach_target(target, width, height)?;

                return Ok(FrameOutcome::Retry);
            }
            Texture::Validation => return Err(GraphicsError::SurfaceValidation),
        };

        let mut frame = Frame {
            view: texture.texture.create_view(&Default::default()),
            encoder: self.device.create_command_encoder(&Default::default()),
        };

        draw(&mut frame).map_err(GraphicsError::Draw)?;

        self.queue.submit([frame.encoder.finish()]);

        before_present();

        self.queue.present(texture);

        if reconfigure {
            let configuration = self
                .presentation
                .as_ref()
                .ok_or(GraphicsError::NotPresenting)?
                .config
                .clone();

            self.configure_presentation(configuration)?;
        }

        self.check_errors()?;

        Ok(FrameOutcome::Presented)
    }
}
