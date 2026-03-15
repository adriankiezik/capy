use std::marker::PhantomData;

use bytemuck::Pod;
use wgpu::util::DeviceExt;

pub(crate) struct UniformBuffer<T: Pod> {
    buffer: wgpu::Buffer,
    _marker: PhantomData<T>,
}

impl<T: Pod> UniformBuffer<T> {
    pub(crate) fn new(device: &wgpu::Device, label: &str, initial: &T) -> Self {
        let buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some(label),
            contents: bytemuck::bytes_of(initial),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        Self {
            buffer,
            _marker: PhantomData,
        }
    }

    pub(crate) fn write(&self, queue: &wgpu::Queue, value: &T) {
        queue.write_buffer(&self.buffer, 0, bytemuck::bytes_of(value));
    }

    pub(crate) fn buffer(&self) -> &wgpu::Buffer {
        &self.buffer
    }
}
