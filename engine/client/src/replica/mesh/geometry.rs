use crate::{
    replica::mesh::{MeshError, Result, halo::Halo, work::Work},
    replica::world::{Leaf, VOXEL_SIZE, Voxel, VoxelCoord, WorldRead},
};
use glam::{IVec3, Vec3};
use std::sync::{Arc, Mutex, OnceLock};

pub(crate) type MeshSources = im::OrdMap<[i32; 3], Arc<MeshSource>>;

#[derive(Default)]
pub(crate) struct Scratch {
    halo: Halo,
    mask: Vec<Face>,
}

#[derive(Debug, Default)]
pub(crate) struct MeshSource {
    pub(crate) mesh: OnceLock<Arc<Mesh>>,
    fallback: Mutex<Option<Arc<Mesh>>>,
    pub(crate) queued: std::sync::atomic::AtomicBool,
    pub(crate) blocked: Mutex<Option<(usize, usize)>>,
    pub(crate) failed: std::sync::atomic::AtomicBool,
}

impl MeshSource {
    pub(crate) fn replacement(previous: &Self) -> Self {
        Self {
            fallback: Mutex::new(previous.ready().or_else(|| previous.fallback())),
            ..Self::default()
        }
    }

    pub(crate) fn fallback(&self) -> Option<Arc<Mesh>> {
        self.fallback
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    pub(crate) fn ready(&self) -> Option<Arc<Mesh>> {
        let mesh = self.mesh.get().cloned();

        if mesh.is_some() {
            self.fallback
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .take();
        }

        mesh
    }

    pub(crate) fn resolve<'a>(
        &self,
        key: [i32; 3],
        edge: i32,
        leaf: impl Fn([i32; 3]) -> Option<&'a Leaf>,
        world: &WorldRead,
        maximum: usize,
        scratch: &mut Scratch,
    ) -> Result<Arc<Mesh>> {
        if let Some(mesh) = self.mesh.get() {
            if mesh.vertices.len() > maximum {
                return Err(MeshError::Limit("resident mesh vertices"));
            }

            return Ok(mesh.clone());
        }

        scratch.halo.fill(IVec3::from_array(key) * edge, edge, leaf);

        pollster::block_on(build_halo(key, edge, scratch, world, maximum, true)).map(Arc::new)
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub(crate) struct Vertex {
    pub(crate) position: [f32; 3],
    pub(crate) normal: [f32; 3],
    pub(crate) color: [f32; 3],
    pub(crate) occlusion: f32,
}

#[derive(Debug)]
pub(crate) struct Mesh {
    pub(crate) vertices: Vec<Vertex>,
    pub(crate) min: Vec3,
    pub(crate) max: Vec3,
    pub(crate) allocation: OnceLock<Arc<std::sync::atomic::AtomicUsize>>,
}

impl Drop for Mesh {
    fn drop(&mut self) {
        if let Some(budget) = self.allocation.get() {
            budget.fetch_sub(self.vertices.len(), std::sync::atomic::Ordering::AcqRel);
        }
    }
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
    ambient_occlusion: bool,
    scratch: &mut Scratch,
) -> Result<Mesh> {
    scratch
        .halo
        .sample(IVec3::from_array(key) * edge, edge, sample);

    pollster::block_on(build_halo(
        key,
        edge,
        scratch,
        world,
        maximum,
        ambient_occlusion,
    ))
}

async fn build_halo(
    key: [i32; 3],
    edge: i32,
    scratch: &mut Scratch,
    world: &WorldRead,
    maximum: usize,
    ambient_occlusion: bool,
) -> Result<Mesh> {
    let origin = IVec3::from_array(key) * edge;

    let mut mesh = Mesh {
        vertices: Vec::new(),
        allocation: OnceLock::new(),
        min: Vec3::splat(f32::INFINITY),
        max: Vec3::splat(f32::NEG_INFINITY),
    };

    let n = edge as usize;

    scratch.mask.resize(n * n, Face::EMPTY);

    let mask = &mut scratch.mask;

    let halo = &scratch.halo;

    let mut work = Work::default();

    for axis in 0..3 {
        for sign in [-1, 1] {
            for slice in 0..edge {
                work.checkpoint().await;

                fill_mask(
                    mask,
                    edge,
                    origin,
                    (axis, sign, slice),
                    halo,
                    ambient_occlusion,
                );

                for j in 0..n {
                    let mut i = 0;

                    while i < n {
                        let Some(rectangle) = take_rectangle(mask, n, i, j) else {
                            i += 1;

                            continue;
                        };

                        let color = *world
                            .material(rectangle.face.voxel.material)
                            .ok_or(MeshError::Invalid)?;

                        emit_quad(
                            &mut mesh,
                            rectangle.corners(axis, sign, slice),
                            axis,
                            sign,
                            color,
                            rectangle.face.occlusion,
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

#[derive(Clone, Copy, PartialEq, Eq)]
struct Face {
    voxel: Voxel,
    occlusion: [u8; 4],
}

impl Face {
    const EMPTY: Self = Self {
        voxel: Voxel::EMPTY,
        occlusion: [3; 4],
    };

    fn merges(self, other: Self, horizontal: bool) -> bool {
        let [a, b, c, d] = self.occlusion;

        self.voxel.material == other.voxel.material
            && self.occlusion == other.occlusion
            && if horizontal {
                a == b && c == d
            } else {
                a == d && b == c
            }
    }
}

fn fill_mask(
    mask: &mut [Face],
    edge: i32,
    origin: IVec3,
    (axis, sign, slice): (usize, i32, i32),
    halo: &Halo,
    ambient_occlusion: bool,
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

            let voxel = halo.voxel(p);

            mask[i as usize + j as usize * edge as usize] =
                if !voxel.is_empty() && halo.voxel(p + normal).is_empty() {
                    Face {
                        voxel,
                        occlusion: if ambient_occlusion {
                            halo.occlusion(p, axis, sign)
                        } else {
                            [3; 4]
                        },
                    }
                } else {
                    Face::EMPTY
                };
        }
    }
}

struct Rectangle {
    face: Face,
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

fn take_rectangle(mask: &mut [Face], n: usize, i: usize, j: usize) -> Option<Rectangle> {
    let face = mask[i + j * n];

    if face.voxel.is_empty() {
        return None;
    }

    let mut width = 1;

    while i + width < n && face.merges(mask[i + width + j * n], true) {
        width += 1;
    }

    let mut height = 1;

    while j + height < n && (0..width).all(|x| face.merges(mask[i + x + (j + height) * n], false)) {
        height += 1;
    }

    for y in 0..height {
        for x in 0..width {
            mask[i + x + (j + y) * n] = Face::EMPTY;
        }
    }

    Some(Rectangle {
        face,
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
    occlusion: [u8; 4],
    maximum: usize,
) -> Result<()> {
    if mesh.vertices.len() + 6 > maximum {
        return Err(MeshError::Limit("mesh vertices"));
    }

    let mut normal = Vec3::ZERO;

    normal[axis] = sign as f32;

    let mut order = if occlusion[0] + occlusion[2] > occlusion[1] + occlusion[3] {
        [0, 1, 3, 1, 2, 3]
    } else {
        [0, 1, 2, 0, 2, 3]
    };

    if sign < 0 {
        order.swap(1, 2);

        order.swap(4, 5);
    }

    for index in order {
        let p = corners[index];

        mesh.min = mesh.min.min(p);
        mesh.max = mesh.max.max(p);

        mesh.vertices.push(Vertex {
            position: p.to_array(),
            normal: normal.to_array(),
            color,
            occlusion: occlusion[index] as f32 / 3.0,
        });
    }

    Ok(())
}
