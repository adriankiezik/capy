use glam::Vec3;
use wgpu::util::DeviceExt;

use capy_core::Camera;
use capy_render::SharedVoxelBuffers;

use super::VoxelHit;

const PICK_SHADER: &str = include_str!("../shaders/pick.wgsl");
pub(crate) const PICK_OUTPUT_SIZE: u64 = 32;

#[repr(C)]
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
pub(crate) struct PickInputUniform {
    pub pixel_x: u32,
    pub pixel_y: u32,
}

pub(crate) struct PickPipeline {
    pub compute_pipeline: wgpu::ComputePipeline,
    pub bind_group: wgpu::BindGroup,

    pub pick_input_buffer: wgpu::Buffer,
    pub pick_output_buffer: wgpu::Buffer,
    pub pick_staging_buffer: wgpu::Buffer,
    pub camera_buffer: wgpu::Buffer,

    pub pending_rx: Option<std::sync::mpsc::Receiver<Result<(), wgpu::BufferAsyncError>>>,
}

impl PickPipeline {
    pub(crate) fn new(
        device: &wgpu::Device,
        camera: &Camera,
        width: u32,
        height: u32,
        voxels: &SharedVoxelBuffers,
    ) -> Self {
        let camera_buffer = capy_render::create_camera_buffer(device, camera, width, height, 0.0);

        let pick_input = PickInputUniform {
            pixel_x: 0,
            pixel_y: 0,
        };
        let pick_input_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Pick Input"),
            contents: bytemuck::bytes_of(&pick_input),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        let pick_output_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Pick Output"),
            size: PICK_OUTPUT_SIZE,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });

        let pick_staging_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Pick Staging"),
            size: PICK_OUTPUT_SIZE,
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let shader_module =
            capy_render::create_compute_shader(device, "Pick Compute Shader", PICK_SHADER);

        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Pick BGL"),
            entries: &[
                bgl_uniform(0),
                bgl_uniform(1),
                bgl_storage_ro(2),
                bgl_storage_ro(3),
                bgl_storage_ro(4),
                bgl_uniform(5),
                bgl_uniform(6),
                bgl_storage_rw(7),
            ],
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Pick Pipeline Layout"),
            bind_group_layouts: &[&bind_group_layout],
            ..Default::default()
        });

        let compute_pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("Pick Pipeline"),
            layout: Some(&pipeline_layout),
            module: &shader_module,
            entry_point: Some("main"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            cache: None,
        });

        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Pick Bind Group"),
            layout: &bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: camera_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: voxels.streaming_info_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: voxels.pool_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: voxels.avg_pool_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: voxels.indirection_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 5,
                    resource: voxels.render_settings_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 6,
                    resource: pick_input_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 7,
                    resource: pick_output_buffer.as_entire_binding(),
                },
            ],
        });

        Self {
            compute_pipeline,
            bind_group,
            pick_input_buffer,
            pick_output_buffer,
            pick_staging_buffer,
            camera_buffer,
            pending_rx: None,
        }
    }

    pub(crate) fn try_read_result(&mut self) -> Option<VoxelHit> {
        let rx = self.pending_rx.take()?;

        match rx.try_recv() {
            Ok(Ok(())) => {
                let data = self.pick_staging_buffer.slice(..).get_mapped_range();
                let values: &[f32] = bytemuck::cast_slice(&data);
                let hit_bits = bytemuck::cast::<f32, u32>(values[0]);
                let material_bits = bytemuck::cast::<f32, u32>(values[7]);

                let hit = hit_bits != 0;
                let result = if hit {
                    VoxelHit {
                        hit: true,
                        position: Vec3::new(values[1], values[2], values[3]),
                        normal: Vec3::new(values[4], values[5], values[6]),
                        material: material_bits,
                    }
                } else {
                    VoxelHit::default()
                };

                drop(data);
                self.pick_staging_buffer.unmap();
                Some(result)
            }
            Ok(Err(_)) => None,
            Err(std::sync::mpsc::TryRecvError::Empty) => {
                self.pending_rx = Some(rx);
                None
            }
            Err(std::sync::mpsc::TryRecvError::Disconnected) => None,
        }
    }
}

fn bgl_uniform(binding: u32) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::COMPUTE,
        ty: wgpu::BindingType::Buffer {
            ty: wgpu::BufferBindingType::Uniform,
            has_dynamic_offset: false,
            min_binding_size: None,
        },
        count: None,
    }
}

fn bgl_storage_ro(binding: u32) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::COMPUTE,
        ty: wgpu::BindingType::Buffer {
            ty: wgpu::BufferBindingType::Storage { read_only: true },
            has_dynamic_offset: false,
            min_binding_size: None,
        },
        count: None,
    }
}

fn bgl_storage_rw(binding: u32) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::COMPUTE,
        ty: wgpu::BindingType::Buffer {
            ty: wgpu::BufferBindingType::Storage { read_only: false },
            has_dynamic_offset: false,
            min_binding_size: None,
        },
        count: None,
    }
}
