use crate::world::{
    Material, MaterialId, Owner, OwnerId, Result, Support, Voxel, VoxelCoord, WorldError,
    WorldSettings,
};
use glam::IVec3;
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};

pub(crate) const LEAF_EDGE: i32 = 8;

pub(crate) const LEAF_VOXELS: usize = (LEAF_EDGE * LEAF_EDGE * LEAF_EDGE) as usize;

pub(crate) type LeafCoord = [i32; 3];

type RegionCoord = [i32; 2];

type Region = BTreeMap<LeafCoord, Arc<Leaf>>;

#[derive(Debug)]
pub(crate) enum Leaf {
    Uniform(Voxel),
    Dense(Box<[Voxel]>),
    Palette {
        values: Box<[Voxel]>,
        indices: Box<[u16]>,
    },
}

impl Leaf {
    pub(crate) fn encode(voxels: &[Voxel]) -> Option<Self> {
        if voxels.iter().all(|c| c.is_empty()) {
            return None;
        }

        let mut palette = BTreeMap::new();

        let mut values = Vec::new();

        let indices = voxels
            .iter()
            .map(|&voxel| {
                *palette.entry(voxel).or_insert_with(|| {
                    let index = values.len() as u16;

                    values.push(voxel);

                    index
                })
            })
            .collect::<Vec<_>>();

        if values.len() == 1 {
            Some(Self::Uniform(values[0]))
        } else if std::mem::size_of_val(values.as_slice())
            + std::mem::size_of_val(indices.as_slice())
            >= std::mem::size_of_val(voxels)
        {
            Some(Self::Dense(voxels.into()))
        } else {
            Some(Self::Palette {
                values: values.into_boxed_slice(),
                indices: indices.into_boxed_slice(),
            })
        }
    }

    pub(crate) fn voxel(&self, index: usize) -> Voxel {
        match self {
            Self::Uniform(voxel) => *voxel,
            Self::Dense(voxels) => voxels[index],
            Self::Palette { values, indices } => values[indices[index] as usize],
        }
    }

    fn owners(&self) -> BTreeSet<OwnerId> {
        let values = match self {
            Self::Uniform(voxel) => std::slice::from_ref(voxel),
            Self::Dense(voxels) => voxels,
            Self::Palette { values, .. } => values,
        };

        values
            .iter()
            .filter_map(|voxel| (!voxel.is_empty()).then_some(voxel.owner))
            .collect()
    }

    pub(crate) fn decode(&self) -> Vec<Voxel> {
        (0..LEAF_VOXELS).map(|i| self.voxel(i)).collect()
    }
}

pub(crate) fn address(voxel: VoxelCoord) -> (LeafCoord, usize) {
    let key = voxel.to_array().map(|v| v.div_euclid(LEAF_EDGE));

    let [x, y, z] = voxel.to_array().map(|v| v.rem_euclid(LEAF_EDGE) as usize);

    (key, x + LEAF_EDGE as usize * (y + LEAF_EDGE as usize * z))
}

pub(crate) fn coordinate(key: LeafCoord, index: usize) -> VoxelCoord {
    let edge = LEAF_EDGE as usize;

    IVec3::from_array(key) * LEAF_EDGE
        + IVec3::new(
            (index % edge) as i32,
            (index / edge % edge) as i32,
            (index / (edge * edge)) as i32,
        )
}

fn region(key: LeafCoord) -> RegionCoord {
    [key[0].div_euclid(40), key[2].div_euclid(40)]
}

#[derive(Clone, Debug)]
pub struct WorldRead {
    pub(crate) root: Arc<Root>,
}

#[derive(Clone, Debug)]
pub(crate) struct Root {
    pub(crate) epoch: u64,
    pub(crate) regions: BTreeMap<RegionCoord, Arc<Region>>,
    pub(crate) settings: Arc<WorldSettings>,
    pub(crate) materials: Arc<BTreeMap<MaterialId, Material>>,
    pub(crate) owners: Arc<BTreeMap<OwnerId, Owner>>,
    owner_leaves: BTreeMap<OwnerId, Arc<BTreeSet<LeafCoord>>>,
}

impl WorldRead {
    pub fn new(settings: WorldSettings) -> Result<Self> {
        let bounds = settings.bounds;

        if !bounds.min.cmplt(bounds.max).all()
            || bounds.min.cmplt(IVec3::splat(-1_000_000)).any()
            || bounds.max.cmpgt(IVec3::splat(1_000_000)).any()
            || settings.max_leaves == 0
            || settings.max_edit_voxels == 0
            || settings.max_support_voxels == 0
        {
            return Err(WorldError::Invalid);
        }

        let mut materials = BTreeMap::new();

        for material in &settings.materials {
            if material.id.0 == 0
                || material
                    .color
                    .iter()
                    .any(|v| !v.is_finite() || !(0.0..=1.0).contains(v))
                || !material.density.is_finite()
                || material.density <= 0.0
                || materials.insert(material.id, material.clone()).is_some()
            {
                return Err(WorldError::Invalid);
            }
        }

        let mut owners = BTreeMap::new();

        for owner in &settings.owners {
            if owner.id.0 == 0
                || owner.structure.0 == 0
                || owners.insert(owner.id, owner.clone()).is_some()
            {
                return Err(WorldError::Invalid);
            }
        }

        for owner in owners.values() {
            if let Support::Contact(id) = owner.support
                && !owners.get(&id).is_some_and(|o| o.support == Support::Fixed)
            {
                return Err(WorldError::Invalid);
            }
        }

        Ok(Self {
            root: Arc::new(Root {
                epoch: 0,
                regions: BTreeMap::new(),
                settings: Arc::new(settings),
                materials: Arc::new(materials),
                owners: Arc::new(owners),
                owner_leaves: BTreeMap::new(),
            }),
        })
    }

    pub fn epoch(&self) -> u64 {
        self.root.epoch
    }

    pub fn bounds(&self) -> crate::world::VoxelBounds {
        self.root.settings.bounds
    }

    pub fn voxel(&self, voxel: VoxelCoord) -> Result<Voxel> {
        if !self.bounds().contains(voxel) {
            return Err(WorldError::Unavailable(voxel));
        }

        Ok(self.resident_voxel(voxel))
    }

    pub fn owner(&self, id: OwnerId) -> Option<&Owner> {
        self.root.owners.get(&id)
    }

    pub fn material(&self, id: MaterialId) -> Option<&Material> {
        self.root.materials.get(&id)
    }

    pub fn leaf_count(&self) -> usize {
        self.root.regions.values().map(|r| r.len()).sum()
    }

    pub(crate) fn resident_voxel(&self, voxel: VoxelCoord) -> Voxel {
        let (key, index) = address(voxel);

        self.leaf(key)
            .map_or(Voxel::EMPTY, |leaf| leaf.voxel(index))
    }

    pub(crate) fn has_render_voxels(&self, key: [i32; 3], edge: i32) -> bool {
        let count = edge / LEAF_EDGE;

        let origin = IVec3::from_array(key) * count;

        (0..count).any(|z| {
            (0..count).any(|y| {
                (0..count).any(|x| {
                    self.leaf((origin + IVec3::new(x, y, z)).to_array())
                        .is_some()
                })
            })
        })
    }

    pub(crate) fn leaf(&self, key: LeafCoord) -> Option<&Arc<Leaf>> {
        self.root.regions.get(&region(key))?.get(&key)
    }

    pub fn owner_voxels(&self, owner: OwnerId) -> impl Iterator<Item = (VoxelCoord, Voxel)> + '_ {
        self.root
            .owner_leaves
            .get(&owner)
            .into_iter()
            .flat_map(|keys| keys.iter())
            .filter_map(|&key| self.leaf(key).map(|leaf| (key, leaf)))
            .flat_map(move |(key, leaf)| {
                (0..LEAF_VOXELS).filter_map(move |i| {
                    let voxel = leaf.voxel(i);

                    (voxel.owner == owner).then_some((coordinate(key, i), voxel))
                })
            })
    }

    pub(crate) fn replace_leaves(&self, changes: BTreeMap<LeafCoord, Vec<Voxel>>) -> Result<Self> {
        let mut root = (*self.root).clone();

        root.epoch = root
            .epoch
            .checked_add(1)
            .ok_or(WorldError::Limit("epochs"))?;

        for (key, voxels) in changes {
            if let Some(old) = self.leaf(key) {
                for owner in old.owners() {
                    if let Some(keys) = root.owner_leaves.get_mut(&owner) {
                        Arc::make_mut(keys).remove(&key);
                    }
                }
            }

            let r = Arc::make_mut(root.regions.entry(region(key)).or_default());

            if let Some(leaf) = Leaf::encode(&voxels) {
                for owner in leaf.owners() {
                    Arc::make_mut(root.owner_leaves.entry(owner).or_default()).insert(key);
                }

                r.insert(key, Arc::new(leaf));
            } else {
                r.remove(&key);
            }
        }

        root.regions.retain(|_, r| !r.is_empty());

        root.owner_leaves.retain(|_, keys| !keys.is_empty());

        let read = Self {
            root: Arc::new(root),
        };

        if read.leaf_count() > read.root.settings.max_leaves {
            return Err(WorldError::Limit("storage leaves"));
        }

        Ok(read)
    }
}
