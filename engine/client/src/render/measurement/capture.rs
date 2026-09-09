use super::{CapturePaths, Result};
use crate::camera::Camera;
use std::sync::mpsc;

pub(super) fn capture(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    color: &wgpu::Texture,
    depth: &wgpu::Texture,
    camera: Camera,
    far: f32,
    paths: Option<&CapturePaths>,
) -> Result<(String, String)> {
    let stride = (color.width() * 4).div_ceil(256) * 256;

    let buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("measurement capture"),
        size: stride as u64 * color.height() as u64,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });

    let pixels = texture_bytes(
        device,
        queue,
        color,
        wgpu::TextureAspect::All,
        &buffer,
        stride,
    )?;

    let depths = texture_bytes(
        device,
        queue,
        depth,
        wgpu::TextureAspect::DepthOnly,
        &buffer,
        stride,
    )?;

    if let Some(paths) = paths {
        std::fs::write(&paths.rgba8_srgb, &pixels)?;

        std::fs::write(&paths.depth_f32_le, &depths)?;

        let matrix = camera
            .matrix(color.width() as f32 / color.height() as f32, far)
            .ok_or_else(|| anyhow::anyhow!("invalid capture camera"))?;

        let mut data = matrix.to_cols_array().to_vec();

        data.extend_from_slice(&camera.position.to_array());

        std::fs::write(
            &paths.camera_columns_eye_f32_le,
            data.into_iter()
                .flat_map(f32::to_le_bytes)
                .collect::<Vec<_>>(),
        )?;
    }

    Ok((hash(&pixels), hash(&depths)))
}

fn texture_bytes(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    texture: &wgpu::Texture,
    aspect: wgpu::TextureAspect,
    buffer: &wgpu::Buffer,
    stride: u32,
) -> Result<Vec<u8>> {
    let mut encoder = device.create_command_encoder(&Default::default());

    let mut source = texture.as_image_copy();

    source.aspect = aspect;

    encoder.copy_texture_to_buffer(
        source,
        wgpu::TexelCopyBufferInfo {
            buffer,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(stride),
                rows_per_image: None,
            },
        },
        texture.size(),
    );

    queue.submit([encoder.finish()]);

    let bytes = read(device, buffer)?;

    Ok(bytes
        .chunks_exact(stride as usize)
        .flat_map(|row| row[..texture.width() as usize * 4].iter().copied())
        .collect())
}

fn hash(bytes: &[u8]) -> String {
    let value = bytes.iter().fold(0xcbf29ce484222325u64, |hash, byte| {
        (hash ^ *byte as u64).wrapping_mul(0x100000001b3)
    });

    format!("{value:016x}")
}

pub(super) fn read(device: &wgpu::Device, buffer: &wgpu::Buffer) -> Result<Vec<u8>> {
    let (sender, receiver) = mpsc::channel();

    buffer.map_async(wgpu::MapMode::Read, .., move |result| {
        let _ = sender.send(result);
    });

    device.poll(wgpu::PollType::Wait {
        submission_index: None,
        timeout: Some(std::time::Duration::from_secs(60)),
    })?;

    receiver.recv()??;

    let data = buffer.get_mapped_range(..)?.to_vec();

    buffer.unmap();

    Ok(data)
}
