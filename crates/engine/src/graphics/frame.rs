use crate::graphics::{FrameOutcome, Graphics, GraphicsError, Result};

#[derive(Debug)]
pub struct Frame<'a> {
    texture: &'a wgpu::Texture,
    view: wgpu::TextureView,
    encoder: wgpu::CommandEncoder,
}

impl Frame<'_> {
    pub fn parts(&mut self) -> (&wgpu::TextureView, &mut wgpu::CommandEncoder) {
        (&self.view, &mut self.encoder)
    }

    pub fn texture(&self) -> &wgpu::Texture {
        self.texture
    }
}

impl Graphics {
    pub(crate) fn render(
        &mut self,
        draw: impl FnOnce(&mut Frame<'_>) -> anyhow::Result<()>,
        before_present: impl FnOnce(),
    ) -> Result<FrameOutcome> {
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

        self.checked(|device, queue| -> Result<()> {
            let mut frame = Frame {
                texture: &texture.texture,
                view: texture.texture.create_view(&Default::default()),
                encoder: device.create_command_encoder(&Default::default()),
            };

            draw(&mut frame).map_err(GraphicsError::Draw)?;

            queue.submit([frame.encoder.finish()]);

            Ok(())
        })??;

        self.checked(|_, queue| {
            before_present();

            queue.present(texture);
        })?;

        if reconfigure {
            let configuration = self
                .presentation
                .as_ref()
                .ok_or(GraphicsError::NotPresenting)?
                .config
                .clone();

            self.configure_presentation(configuration)?;
        }

        Ok(FrameOutcome::Presented)
    }
}
