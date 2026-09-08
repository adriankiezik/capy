use crate::Aabb;
use glam::{IVec3, Vec3};

pub const VOXEL_SIZE: f32 = 0.1;

pub type VoxelCoord = IVec3;

pub fn voxel_bounds(coordinate: VoxelCoord) -> Aabb {
    let min = coordinate.as_vec3() * VOXEL_SIZE;

    Aabb {
        min,
        max: min + Vec3::splat(VOXEL_SIZE),
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct OwnerId(pub u64);

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct StructureId(pub u64);

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct MaterialId(pub u16);

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub struct Voxel {
    pub material: MaterialId,
    pub owner: OwnerId,
}

impl Voxel {
    pub const EMPTY: Self = Self {
        material: MaterialId(0),
        owner: OwnerId(0),
    };

    pub fn new(material: MaterialId, owner: OwnerId) -> Self {
        Self { material, owner }
    }

    pub fn is_empty(self) -> bool {
        self == Self::EMPTY
    }
}

#[derive(Clone, Debug)]
pub struct Material {
    pub id: MaterialId,
    pub color: [f32; 3],
    pub density: f32,
}

#[derive(Clone, Debug)]
pub struct Owner {
    pub id: OwnerId,
    pub structure: StructureId,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct VoxelBounds {
    pub min: VoxelCoord,
    pub max: VoxelCoord,
}

impl VoxelBounds {
    pub fn iter(self) -> impl Iterator<Item = VoxelCoord> {
        (self.min.z..self.max.z).flat_map(move |z| {
            (self.min.y..self.max.y)
                .flat_map(move |y| (self.min.x..self.max.x).map(move |x| IVec3::new(x, y, z)))
        })
    }

    pub fn contains(self, voxel: VoxelCoord) -> bool {
        voxel.cmpge(self.min).all() && voxel.cmplt(self.max).all()
    }

    pub fn intersects(self, min: Vec3, max: Vec3) -> bool {
        min.cmplt(self.max.as_vec3() * VOXEL_SIZE).all()
            && max.cmpgt(self.min.as_vec3() * VOXEL_SIZE).all()
    }
}
