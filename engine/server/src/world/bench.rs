#![allow(clippy::expect_used)]

use super::{
    LEAF_VOXELS, Leaf, Material, MaterialId, Owner, OwnerId, StructureId, Voxel, VoxelBounds,
    WorldRead, WorldSettings,
};
use divan::{Bencher, black_box, counter::ItemsCount};
use glam::IVec3;
use std::collections::BTreeMap;

fn populated_world(leaves: i32) -> WorldRead {
    let world = WorldRead::new(WorldSettings {
        bounds: VoxelBounds {
            min: IVec3::ZERO,
            max: IVec3::new(leaves * 8, 16, 8),
        },
        materials: vec![Material {
            id: MaterialId(1),

            density: 1.0,
        }],
        owners: vec![Owner {
            id: OwnerId(1),
            structure: StructureId(1),
        }],
        max_leaves: leaves as usize,
        max_edit_voxels: 65536,
        max_support_voxels: 65536,
    })
    .expect("valid world fixture");

    let changes = (0..leaves)
        .map(|x| ([x, 0, 0], voxels(1)))
        .collect::<BTreeMap<_, _>>();

    world
        .replace_leaves(changes)
        .expect("valid populated world")
}

fn world_lookup(bencher: Bencher, leaves: i32, scattered: bool, missing: bool) {
    let world = populated_world(leaves);

    let positions = (0..leaves * 8)
        .map(|index| {
            let x = if scattered {
                (index * 137) % (leaves * 8)
            } else {
                index % (leaves * 8)
            };

            IVec3::new(x, if missing { 8 } else { 0 }, 0)
        })
        .collect::<Vec<_>>();

    assert_eq!(
        world
            .voxel(positions[0])
            .expect("valid coordinate")
            .is_empty(),
        missing
    );

    bencher
        .counter(ItemsCount::new(positions.len()))
        .bench_local(|| {
            for &position in black_box(&positions) {
                black_box(black_box(&world).voxel(position).expect("valid coordinate"));
            }
        });
}

#[divan::bench(args = [8, 128, 1024])]
fn voxel_sequential(bencher: Bencher, leaves: i32) {
    world_lookup(bencher, leaves, false, false);
}

#[divan::bench(args = [8, 128, 1024])]
fn voxel_scattered(bencher: Bencher, leaves: i32) {
    world_lookup(bencher, leaves, true, false);
}

#[divan::bench(args = [8, 128, 1024])]
fn voxel_missing(bencher: Bencher, leaves: i32) {
    world_lookup(bencher, leaves, true, true);
}

#[divan::bench(args = [8, 128, 1024])]
fn owner_iteration(bencher: Bencher, leaves: i32) {
    let world = populated_world(leaves);

    bencher
        .counter(ItemsCount::new(leaves as usize * LEAF_VOXELS))
        .bench_local(|| {
            for voxel in black_box(&world).owner_voxels(black_box(OwnerId(1))) {
                black_box(voxel);
            }
        });
}

fn voxels(distinct: usize) -> Vec<Voxel> {
    (0..LEAF_VOXELS)
        .map(|index| Voxel::new(MaterialId(1), OwnerId((index % distinct + 1) as u64)))
        .collect()
}

#[divan::bench(args = [1, 4, 64, 512])]
fn leaf_encode(bencher: Bencher, distinct: usize) {
    let voxels = voxels(distinct);

    bencher
        .counter(ItemsCount::new(LEAF_VOXELS))
        .bench_local(|| Leaf::encode(black_box(&voxels)));
}

#[divan::bench(args = [1, 4, 64, 512])]
fn leaf_decode(bencher: Bencher, distinct: usize) {
    let leaf = Leaf::encode(&voxels(distinct)).expect("nonempty leaf fixture");

    bencher
        .counter(ItemsCount::new(LEAF_VOXELS))
        .bench_local(|| black_box(&leaf).decode());
}

#[divan::bench(args = [1, 4, 64, 512])]
fn leaf_lookup(bencher: Bencher, distinct: usize) {
    let leaf = Leaf::encode(&voxels(distinct)).expect("nonempty leaf fixture");

    bencher
        .counter(ItemsCount::new(LEAF_VOXELS))
        .bench_local(|| {
            let leaf = black_box(&leaf);

            for index in 0..LEAF_VOXELS {
                black_box(leaf.voxel(black_box((index * 137) % LEAF_VOXELS)));
            }
        });
}
