use glam::Vec3;
use wgpu::util::DeviceExt;

use capy_core::Camera;
use capy_render::SharedVoxelBuffers;

use super::VoxelHit;

const PICK_SHADER: &str = include_str!("../shaders/pick.wgsl");
pub(crate) const PICK_OUTPUT_SIZE: u64 = 60;

#[repr(C)]
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
pub(crate) struct PickInputUniform {
    pub pixel_x: u32,
    pub pixel_y: u32,
}

pub(crate) struct PickPipeline {
    pub compute_pipeline: wgpu::ComputePipeline,
    bind_group_layout: wgpu::BindGroupLayout,
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

        let mut layout_entries = capy_render::voxel_scene_bind_group_layout_entries();
        layout_entries.push(capy_render::bgl_uniform(6));
        layout_entries.push(capy_render::bgl_storage_rw(7));

        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Pick BGL"),
            entries: &layout_entries,
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

        let bind_group = create_pick_bind_group(
            device,
            &bind_group_layout,
            &camera_buffer,
            &pick_input_buffer,
            &pick_output_buffer,
            voxels,
        );

        Self {
            compute_pipeline,
            bind_group_layout,
            bind_group,
            pick_input_buffer,
            pick_output_buffer,
            pick_staging_buffer,
            camera_buffer,
            pending_rx: None,
        }
    }

    pub(crate) fn rebind(&mut self, device: &wgpu::Device, voxels: &SharedVoxelBuffers) {
        self.bind_group = create_pick_bind_group(
            device,
            &self.bind_group_layout,
            &self.camera_buffer,
            &self.pick_input_buffer,
            &self.pick_output_buffer,
            voxels,
        );
    }

    pub(crate) fn try_read_result(&mut self) -> Option<VoxelHit> {
        let rx = self.pending_rx.take()?;

        match rx.try_recv() {
            Ok(Ok(())) => {
                let data = self.pick_staging_buffer.slice(..).get_mapped_range();
                let values: &[f32] = bytemuck::cast_slice(&data);
                let hit_bits = bytemuck::cast::<f32, u32>(values[0]);
                let material_bits = bytemuck::cast::<f32, u32>(values[7]);
                let water_hit_bits = bytemuck::cast::<f32, u32>(values[8]);

                let hit = hit_bits != 0;
                let water_hit = water_hit_bits != 0;
                let result = VoxelHit {
                    hit,
                    position: if hit {
                        Vec3::new(values[1], values[2], values[3])
                    } else {
                        Vec3::ZERO
                    },
                    normal: if hit {
                        Vec3::new(values[4], values[5], values[6])
                    } else {
                        Vec3::ZERO
                    },
                    material: material_bits,
                    water_hit,
                    water_position: if water_hit {
                        Vec3::new(values[9], values[10], values[11])
                    } else {
                        Vec3::ZERO
                    },
                    water_normal: if water_hit {
                        Vec3::new(values[12], values[13], values[14])
                    } else {
                        Vec3::ZERO
                    },
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

fn create_pick_bind_group(
    device: &wgpu::Device,
    bind_group_layout: &wgpu::BindGroupLayout,
    camera_buffer: &wgpu::Buffer,
    pick_input_buffer: &wgpu::Buffer,
    pick_output_buffer: &wgpu::Buffer,
    voxels: &SharedVoxelBuffers,
) -> wgpu::BindGroup {
    let mut bg_entries = capy_render::voxel_scene_bind_group_entries(camera_buffer, voxels);
    bg_entries.push(wgpu::BindGroupEntry {
        binding: 6,
        resource: pick_input_buffer.as_entire_binding(),
    });
    bg_entries.push(wgpu::BindGroupEntry {
        binding: 7,
        resource: pick_output_buffer.as_entire_binding(),
    });

    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("Pick Bind Group"),
        layout: bind_group_layout,
        entries: &bg_entries,
    })
}
