use wgpu::util::DeviceExt;

use capy_core::Camera;
use glam::{Mat4, Vec3, Vec4};

const NEAR_CLIP: f32 = 0.1;

fn inv_view_proj(camera: &Camera) -> Mat4 {
    let proj = Mat4::perspective_infinite_rh(camera.fov_y, camera.aspect, NEAR_CLIP);
    let view_at_origin = Mat4::look_to_rh(Vec3::ZERO, camera.forward(), Vec3::Y);
    (proj * view_at_origin).inverse()
}

pub(crate) fn clip_from_world(camera: &Camera) -> Mat4 {
    let proj = Mat4::perspective_infinite_rh(camera.fov_y, camera.aspect, NEAR_CLIP);
    let view = Mat4::look_to_rh(camera.position, camera.forward(), Vec3::Y);
    proj * view
}

#[repr(C)]
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
pub(crate) struct CameraUniform {
    pub(crate) inv_view_proj: [f32; 16],
    pub(crate) camera_pos: [f32; 3],
    pub(crate) camera_underwater: f32,
    pub(crate) resolution: [f32; 2],
    pub(crate) lod_bias: f32,
    pub(crate) pixel_size: f32,
    pub(crate) ray_corner: [f32; 3],
    pub(crate) _pad2: f32,
    pub(crate) ray_right: [f32; 3],
    pub(crate) _pad3: f32,
    pub(crate) ray_up: [f32; 3],
    pub(crate) _pad4: f32,
    pub(crate) jitter: [f32; 2],
    pub(crate) time: f32,
    pub(crate) _pad5: f32,
    pub(crate) clip_from_world: [f32; 16],
    pub(crate) prev_clip_from_world: [f32; 16],
}

const _: () = assert!(std::mem::size_of::<CameraUniform>() == 288);

impl CameraUniform {
    pub(crate) fn from_camera(
        camera: &Camera,
        width: u32,
        height: u32,
        lod_bias: f32,
        camera_underwater: bool,
        jitter: [f32; 2],
        prev_clip_from_world: [f32; 16],
        time: f32,
    ) -> Self {
        let ivp = inv_view_proj(camera);
        let clip_from_world = clip_from_world(camera);

        let p_corner = ivp * Vec4::new(-1.0, -1.0, -1.0, 1.0);
        let p_right = ivp * Vec4::new(1.0, -1.0, -1.0, 1.0);
        let p_up = ivp * Vec4::new(-1.0, 1.0, -1.0, 1.0);

        let p1_corner = ivp * Vec4::new(-1.0, -1.0, 0.0, 1.0);
        let p1_right = ivp * Vec4::new(1.0, -1.0, 0.0, 1.0);
        let p1_up = ivp * Vec4::new(-1.0, 1.0, 0.0, 1.0);

        let w0_corner = p_corner.truncate() / p_corner.w;
        let w1_corner = p1_corner.truncate() / p1_corner.w;
        let dir_corner = (w1_corner - w0_corner).normalize();

        let w0_right = p_right.truncate() / p_right.w;
        let w1_right = p1_right.truncate() / p1_right.w;
        let dir_right = (w1_right - w0_right).normalize();

        let w0_up = p_up.truncate() / p_up.w;
        let w1_up = p1_up.truncate() / p1_up.w;
        let dir_up = (w1_up - w0_up).normalize();

        let ray_right = (dir_right - dir_corner) * 0.5;
        let ray_up = (dir_up - dir_corner) * 0.5;

        Self {
            inv_view_proj: ivp.to_cols_array(),
            camera_pos: camera.position.to_array(),
            camera_underwater: if camera_underwater { 1.0 } else { 0.0 },
            resolution: [width as f32, height as f32],
            lod_bias,
            pixel_size: 2.0 * (camera.fov_y * 0.5).tan() / height as f32,
            ray_corner: dir_corner.into(),
            _pad2: 0.0,
            ray_right: ray_right.into(),
            _pad3: 0.0,
            ray_up: ray_up.into(),
            _pad4: 0.0,
            jitter,
            time,
            _pad5: 0.0,
            clip_from_world: clip_from_world.to_cols_array(),
            prev_clip_from_world,
        }
    }
}

pub fn create_camera_buffer(
    device: &wgpu::Device,
    camera: &Camera,
    width: u32,
    height: u32,
    lod_bias: f32,
) -> wgpu::Buffer {
    let uniform = CameraUniform::from_camera(
        camera,
        width,
        height,
        lod_bias,
        false,
        [0.0, 0.0],
        clip_from_world(camera).to_cols_array(),
        0.0,
    );
    device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("Camera Uniform"),
        contents: bytemuck::bytes_of(&uniform),
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
    })
}

pub fn write_camera_buffer(
    queue: &wgpu::Queue,
    buffer: &wgpu::Buffer,
    camera: &Camera,
    width: u32,
    height: u32,
    lod_bias: f32,
) {
    let uniform = CameraUniform::from_camera(
        camera,
        width,
        height,
        lod_bias,
        false,
        [0.0, 0.0],
        clip_from_world(camera).to_cols_array(),
        0.0,
    );
    queue.write_buffer(buffer, 0, bytemuck::bytes_of(&uniform));
}
