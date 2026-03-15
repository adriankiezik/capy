use crate::resources::SharedVoxelBuffers;

pub const VOXEL_SCENE_BINDING_COUNT: u32 = 6;

pub fn voxel_scene_bind_group_layout_entries() -> Vec<wgpu::BindGroupLayoutEntry> {
    vec![
        bgl_uniform(0),
        bgl_uniform(1),
        bgl_storage_ro(2),
        bgl_storage_ro(3),
        bgl_storage_ro(4),
        bgl_uniform(5),
    ]
}

pub fn voxel_scene_bind_group_entries<'a>(
    camera_buffer: &'a wgpu::Buffer,
    voxels: &'a SharedVoxelBuffers,
) -> Vec<wgpu::BindGroupEntry<'a>> {
    vec![
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
    ]
}

pub fn bgl_uniform(binding: u32) -> wgpu::BindGroupLayoutEntry {
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

pub fn bgl_storage_ro(binding: u32) -> wgpu::BindGroupLayoutEntry {
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

pub fn bgl_storage_rw(binding: u32) -> wgpu::BindGroupLayoutEntry {
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
