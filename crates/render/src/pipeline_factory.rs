pub(crate) fn create_compute_pipeline_with_layout(
    device: &wgpu::Device,
    label: &str,
    shader_source: &str,
    bind_group_layout: &wgpu::BindGroupLayout,
) -> wgpu::ComputePipeline {
    // SAFETY: these shaders are engine-authored (validated by naga in tests)
    // and index engine-built buffers; skip wgpu's injected per-access bounds
    // clamps and loop bounding, which tax the traversal inner loops.
    let shader = unsafe {
        device.create_shader_module_trusted(
            wgpu::ShaderModuleDescriptor {
                label: Some(&format!("{label} Shader")),
                source: wgpu::ShaderSource::Wgsl(shader_source.into()),
            },
            wgpu::ShaderRuntimeChecks::unchecked(),
        )
    };

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
