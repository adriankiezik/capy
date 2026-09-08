use crate::{
    scene::{Result, SceneError},
    world::{VOXEL_SIZE, Voxel, VoxelCoord, WorldRead},
};
use glam::{IVec3, Vec3};
use std::sync::{Arc, OnceLock};

#[derive(Debug, Default)]
pub(crate) struct MeshSource {
    mesh: OnceLock<Arc<Mesh>>,
}

impl MeshSource {
    pub(crate) fn resolve(
        &self,
        key: [i32; 3],
        edge: i32,
        sample: impl Fn(VoxelCoord) -> Voxel,
        world: &WorldRead,
        maximum: usize,
    ) -> Result<Arc<Mesh>> {
        if let Some(mesh) = self.mesh.get() {
            if mesh.vertices.len() > maximum {
                return Err(SceneError::Limit("resident mesh vertices"));
            }

            return Ok(mesh.clone());
        }

        let mesh = Arc::new(build(key, edge, sample, world, maximum)?);

        Ok(self.mesh.get_or_init(|| mesh).clone())
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub(crate) struct Vertex {
    pub(crate) position: [f32; 3],
    pub(crate) normal: [f32; 3],
    pub(crate) color: [f32; 3],
}

#[derive(Debug)]
pub(crate) struct Mesh {
    pub(crate) vertices: Vec<Vertex>,
    pub(crate) origin: Vec3,
    pub(crate) min: Vec3,
    pub(crate) max: Vec3,
}

pub(crate) fn key(voxel: VoxelCoord, edge: i32) -> [i32; 3] {
    voxel.to_array().map(|v| v.div_euclid(edge))
}

pub(crate) fn build(
    key: [i32; 3],
    edge: i32,
    sample: impl Fn(VoxelCoord) -> Voxel,
    world: &WorldRead,
    maximum: usize,
) -> Result<Mesh> {
    let origin = IVec3::from_array(key) * edge;

    let mut mesh = Mesh {
        vertices: Vec::new(),
        origin: origin.as_vec3() * VOXEL_SIZE,
        min: Vec3::splat(f32::INFINITY),
        max: Vec3::splat(f32::NEG_INFINITY),
    };

    let n = edge as usize;

    let mut mask = vec![Voxel::EMPTY; n * n];

    for axis in 0..3 {
        for sign in [-1, 1] {
            for slice in 0..edge {
                fill_mask(&mut mask, edge, origin, axis, sign, slice, &sample);

                for j in 0..n {
                    let mut i = 0;

                    while i < n {
                        let Some(rectangle) = take_rectangle(&mut mask, n, i, j) else {
                            i += 1;

                            continue;
                        };

                        let color = world
                            .material(rectangle.voxel.material)
                            .ok_or(SceneError::Invalid)?
                            .color;

                        emit_quad(
                            &mut mesh,
                            rectangle.corners(axis, sign, slice),
                            axis,
                            sign,
                            color,
                            maximum,
                        )?;

                        i += rectangle.width;
                    }
                }
            }
        }
    }

    Ok(mesh)
}

fn fill_mask(
    mask: &mut [Voxel],
    edge: i32,
    origin: IVec3,
    axis: usize,
    sign: i32,
    slice: i32,
    sample: &impl Fn(VoxelCoord) -> Voxel,
) {
    let u = (axis + 1) % 3;

    let v = (axis + 2) % 3;

    let mut normal = IVec3::ZERO;

    normal[axis] = sign;

    for j in 0..edge {
        for i in 0..edge {
            let mut p = origin;

            p[axis] += slice;
            p[u] += i;
            p[v] += j;

            let voxel = sample(p);

            mask[i as usize + j as usize * edge as usize] =
                if !voxel.is_empty() && sample(p + normal).is_empty() {
                    voxel
                } else {
                    Voxel::EMPTY
                };
        }
    }
}

struct Rectangle {
    voxel: Voxel,
    i: usize,
    j: usize,
    width: usize,
    height: usize,
}

impl Rectangle {
    fn corners(&self, axis: usize, sign: i32, slice: i32) -> [Vec3; 4] {
        let u = (axis + 1) % 3;

        let v = (axis + 2) % 3;

        let mut p = Vec3::ZERO;

        p[axis] = (slice + i32::from(sign > 0)) as f32;
        p[u] = self.i as f32;
        p[v] = self.j as f32;

        let mut du = Vec3::ZERO;

        let mut dv = Vec3::ZERO;

        du[u] = self.width as f32;
        dv[v] = self.height as f32;

        [p, p + du, p + du + dv, p + dv].map(|p| p * VOXEL_SIZE)
    }
}

fn take_rectangle(mask: &mut [Voxel], n: usize, i: usize, j: usize) -> Option<Rectangle> {
    let voxel = mask[i + j * n];

    if voxel.is_empty() {
        return None;
    }

    let mut width = 1;

    while i + width < n && mask[i + width + j * n].material == voxel.material {
        width += 1;
    }

    let mut height = 1;

    while j + height < n
        && (0..width).all(|x| mask[i + x + (j + height) * n].material == voxel.material)
    {
        height += 1;
    }

    for y in 0..height {
        for x in 0..width {
            mask[i + x + (j + y) * n] = Voxel::EMPTY;
        }
    }

    Some(Rectangle {
        voxel,
        i,
        j,
        width,
        height,
    })
}

fn emit_quad(
    mesh: &mut Mesh,
    corners: [Vec3; 4],
    axis: usize,
    sign: i32,
    color: [f32; 3],
    maximum: usize,
) -> Result<()> {
    if mesh.vertices.len() + 6 > maximum {
        return Err(SceneError::Limit("mesh vertices"));
    }

    let mut normal = Vec3::ZERO;

    normal[axis] = sign as f32;

    let order = if sign > 0 {
        [0, 1, 2, 0, 2, 3]
    } else {
        [0, 2, 1, 0, 3, 2]
    };

    for index in order {
        let p = corners[index];

        mesh.min = mesh.min.min(p);
        mesh.max = mesh.max.max(p);

        mesh.vertices.push(Vertex {
            position: p.to_array(),
            normal: normal.to_array(),
            color,
        });
    }

    Ok(())
}
