pub(crate) fn create_compute_pipeline_with_layout(
    device: &wgpu::Device,
    label: &str,
    shader_source: &str,
    bind_group_layout: &wgpu::BindGroupLayout,
) -> wgpu::ComputePipeline {
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some(&format!("{label} Shader")),
        source: wgpu::ShaderSource::Wgsl(shader_source.into()),
    });

    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some(&format!("{label} Pipeline Layout")),
        bind_group_layouts: &[bind_group_layout],
        ..Default::default()
    });

    device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
        label: Some(&format!("{label} Pipeline")),
        layout: Some(&pipeline_layout),
        module: &shader,
        entry_point: Some("main"),
        compilation_options: wgpu::PipelineCompilationOptions::default(),
        cache: None,
    })
}
