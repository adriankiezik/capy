use super::{ModelConfig, ModelError, error::Result};
use crate::replica::world::{MaterialId, OwnerId, Voxel, VoxelBounds, WorldRead};
use crate::replica::{Mesh, mesh::geometry};
use glam::{IVec3, Mat4, Quat, Vec3};
use std::{cell::Cell, sync::Arc};

pub struct VoxelModel {
    voxel_dimensions: IVec3,
    pub(crate) leaves: Vec<([i32; 3], Vec3, Arc<Mesh>)>,
    pub(crate) vertices: usize,
}

#[derive(Clone)]
pub struct VoxelInstance {
    pub id: u64,
    pub model: Arc<VoxelModel>,
    pub translation: Vec3,
    pub rotation: Quat,
    pub scale: f32,
}

impl VoxelInstance {
    pub fn new(id: u64, model: Arc<VoxelModel>) -> Self {
        Self {
            id,
            model,
            translation: Vec3::ZERO,
            rotation: Quat::IDENTITY,
            scale: 1.0,
        }
    }

    pub(crate) fn matrix(&self) -> Mat4 {
        Mat4::from_scale_rotation_translation(
            Vec3::splat(self.scale),
            self.rotation,
            self.translation,
        )
    }
}

impl VoxelModel {
    pub fn build(
        size: IVec3,
        materials: &[[f32; 3]],
        sample_palette_index: impl Fn(IVec3) -> Option<usize>,
        config: ModelConfig,
    ) -> Result<Arc<Self>> {
        if size.cmple(IVec3::ZERO).any() || size.cmpgt(IVec3::splat(4096)).any() {
            return Err(ModelError::InvalidDimensions(size));
        }

        if ![8, 16, 32, 64].contains(&config.leaf_edge) {
            return Err(ModelError::InvalidLeafEdge(config.leaf_edge));
        }

        if config.max_vertices == 0 {
            return Err(ModelError::EmptyVertexBudget);
        }

        if materials.len() > u16::MAX as usize {
            return Err(ModelError::PaletteTooLarge(materials.len()));
        }

        for (index, color) in materials.iter().enumerate() {
            for (channel, &value) in color.iter().enumerate() {
                if !value.is_finite() || !(0.0..=1.0).contains(&value) {
                    return Err(ModelError::InvalidColor {
                        index,
                        channel,
                        value,
                    });
                }
            }
        }

        let world = WorldRead {
            leaves: Default::default(),
            materials: Arc::new(
                materials
                    .iter()
                    .enumerate()
                    .map(|(i, &color)| (MaterialId(i as u16 + 1), color))
                    .collect(),
            ),
            bounds: VoxelBounds {
                min: IVec3::ZERO,
                max: size,
            },
        };

        let mut model = Self {
            voxel_dimensions: size,
            leaves: Vec::new(),
            vertices: 0,
        };

        let edge = config.leaf_edge;

        let counts = (size + IVec3::splat(edge - 1)) / edge;

        if counts.as_i64vec3().element_product() > 1_000_000 {
            return Err(ModelError::Limit("leaves"));
        }

        let mut scratch = geometry::Scratch::default();

        let invalid_material = Cell::new(None);

        let voxel = |p: IVec3| {
            if p.cmplt(IVec3::ZERO).any() || p.cmpge(size).any() {
                return Voxel::EMPTY;
            }

            match sample_palette_index(p) {
                None => Voxel::EMPTY,
                Some(index) if index < materials.len() => {
                    Voxel::new(MaterialId(index as u16 + 1), OwnerId(1))
                }
                Some(index) => {
                    if invalid_material.get().is_none() {
                        invalid_material.set(Some((p, index)));
                    }

                    Voxel::EMPTY
                }
            }
        };

        for z in 0..counts.z {
            for y in 0..counts.y {
                for x in 0..counts.x {
                    let key = [x, y, z];

                    let mesh = geometry::build(
                        key,
                        edge,
                        voxel,
                        &world,
                        config.max_vertices - model.vertices,
                        config.bake_ambient_occlusion,
                        &mut scratch,
                    );

                    if let Some((position, index)) = invalid_material.get() {
                        return Err(ModelError::UndefinedMaterial {
                            position,
                            index,
                            palette_len: materials.len(),
                        });
                    }

                    let mesh = mesh?;

                    if mesh.vertices.is_empty() {
                        continue;
                    }

                    model.vertices += mesh.vertices.len();

                    model.leaves.push((
                        key,
                        IVec3::from_array(key).as_vec3() * edge as f32 * crate::VOXEL_SIZE,
                        Arc::new(mesh),
                    ));
                }
            }
        }

        Ok(Arc::new(model))
    }

    pub fn voxel_dimensions(&self) -> IVec3 {
        self.voxel_dimensions
    }

    pub fn local_bounds(&self) -> crate::Aabb {
        crate::Aabb {
            min: Vec3::ZERO,
            max: self.voxel_dimensions.as_vec3() * crate::VOXEL_SIZE,
        }
    }

    pub fn vertex_count(&self) -> usize {
        self.vertices
    }

    pub fn leaf_count(&self) -> usize {
        self.leaves.len()
    }
}
