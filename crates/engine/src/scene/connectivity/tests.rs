#![allow(clippy::unwrap_used)]

use super::{Connectivity, graph::Group};
use crate::world::{
    LEAF_VOXELS, MaterialId, OwnerId, StructureId, Transaction, Voxel, WorldRead, address,
    coordinate, prepare, tests::settings,
};
use glam::IVec3;
use proptest::prelude::*;
use std::collections::{BTreeMap, BTreeSet};

type Cells = BTreeMap<[i32; 3], Voxel>;

type Components = BTreeSet<BTreeSet<[i32; 3]>>;

fn neighbors(p: [i32; 3]) -> impl Iterator<Item = [i32; 3]> {
    (0..3).flat_map(move |axis| {
        [-1, 1].map(|sign| {
            let mut q = p;

            q[axis] += sign;

            q
        })
    })
}

fn structure(voxel: Voxel) -> u64 {
    if voxel.owner.0 <= 2 { 1 } else { 3 }
}

fn components(cells: &Cells) -> Components {
    let mut remaining: BTreeSet<_> = cells.keys().copied().collect();

    let mut groups = BTreeSet::new();

    while let Some(seed) = remaining.pop_first() {
        let mut group = BTreeSet::from([seed]);

        let mut pending = vec![seed];

        while let Some(p) = pending.pop() {
            for q in neighbors(p) {
                if cells
                    .get(&q)
                    .is_some_and(|v| structure(*v) == structure(cells[&p]))
                    && remaining.remove(&q)
                {
                    group.insert(q);

                    pending.push(q);
                }
            }
        }

        groups.insert(group);
    }

    groups
}

fn supported(cells: &Cells, floor: i32) -> BTreeSet<[i32; 3]> {
    let mut reached: BTreeSet<_> = cells.keys().filter(|p| p[1] == floor).copied().collect();

    let mut pending: Vec<_> = reached.iter().copied().collect();

    while let Some(p) = pending.pop() {
        for q in neighbors(p) {
            if cells
                .get(&q)
                .is_some_and(|v| structure(*v) == structure(cells[&p]) || q[1] == p[1] + 1)
                && reached.insert(q)
            {
                pending.push(q);
            }
        }
    }

    reached
}

fn world(cells: &Cells, floor: i32) -> WorldRead {
    let mut config = settings();

    config.bounds.min.y = floor;
    config.owners[1].structure = StructureId(1);

    let world = WorldRead::new(config).unwrap();

    let mut transaction = Transaction::new(&world);

    for (&p, &voxel) in cells {
        transaction.place(IVec3::from_array(p), voxel).unwrap();
    }

    prepare(&world, transaction).unwrap().0
}

fn unpack(groups: &[Group]) -> Components {
    groups
        .iter()
        .map(|group| {
            group
                .leaves
                .iter()
                .flat_map(|(&key, &mask)| {
                    (0..LEAF_VOXELS)
                        .filter(move |&i| mask.contains(i))
                        .map(move |i| coordinate(key, i).to_array())
                })
                .collect()
        })
        .collect()
}

fn verify(before: Cells, removed: BTreeSet<[i32; 3]>, floor: i32) {
    let after: Cells = before
        .iter()
        .filter(|(p, _)| !removed.contains(*p))
        .map(|(&p, &v)| (p, v))
        .collect();

    let original = world(&before, floor);

    let mut transaction = Transaction::new(&original);

    for &p in &removed {
        transaction.remove(IVec3::from_array(p)).unwrap();
    }

    let updated = prepare(&original, transaction).unwrap().0;

    let changes: BTreeMap<_, _> = removed
        .iter()
        .map(|&p| {
            let key = address(IVec3::from_array(p)).0;

            (key, updated.leaf(key).cloned())
        })
        .collect();

    let expected_support = supported(&after, floor);

    let unsupported: Cells = after
        .iter()
        .filter(|(p, _)| !expected_support.contains(*p))
        .map(|(&p, &v)| (p, v))
        .collect();

    let expected = components(&unsupported);

    let seeds: Vec<_> = before.keys().map(|&p| IVec3::from_array(p)).collect();

    let mut cache = Connectivity::default();

    for _ in 0..2 {
        let groups = pollster::block_on(cache.unsupported(&updated, &seeds)).unwrap();

        assert_eq!(unpack(&groups), expected);

        for group in &groups {
            let leaves =
                pollster::block_on(cache.extract(&updated, group, |key| updated.leaf(key)))
                    .unwrap();

            let extracted: Cells = leaves
                .iter()
                .flat_map(|(&key, leaf)| {
                    (0..LEAF_VOXELS).filter_map(move |i| {
                        let voxel = leaf.voxel(i);

                        (!voxel.is_empty()).then_some((coordinate(key, i).to_array(), voxel))
                    })
                })
                .collect();

            assert_eq!(
                extracted,
                unpack(std::slice::from_ref(group))
                    .first()
                    .unwrap()
                    .iter()
                    .map(|p| (*p, after[p]))
                    .collect()
            );
        }

        if pollster::block_on(cache.preserves(&original, &changes, |key| original.leaf(key)))
            .unwrap()
        {
            let retained: Components = components(&before)
                .into_iter()
                .filter_map(|group| {
                    let group: BTreeSet<_> = group.difference(&removed).copied().collect();

                    (!group.is_empty()).then_some(group)
                })
                .collect();

            assert_eq!(
                components(&after),
                retained,
                "local fast path changed bond connectivity"
            );

            assert_eq!(
                expected_support,
                supported(&before, floor)
                    .difference(&removed)
                    .copied()
                    .collect(),
                "local fast path changed support"
            );
        }
    }

    let supported_before = supported(&before, floor);

    let static_cells: Cells = supported_before.iter().map(|p| (*p, before[p])).collect();

    let static_world = world(&static_cells, floor);

    let cuts: Vec<_> = removed
        .intersection(&supported_before)
        .map(|&p| IVec3::from_array(p))
        .collect();

    let mut transaction = Transaction::new(&static_world);

    for &p in &cuts {
        transaction.remove(p).unwrap();
    }

    let static_after = prepare(&static_world, transaction).unwrap().0;

    let remaining: Cells = static_cells
        .into_iter()
        .filter(|(p, _)| !removed.contains(p))
        .collect();

    let supported_after = supported(&remaining, floor);

    let expected = components(
        &remaining
            .into_iter()
            .filter(|(p, _)| !supported_after.contains(p))
            .collect(),
    );

    for _ in 0..2 {
        let groups = pollster::block_on(cache.unsupported(&static_after, &cuts)).unwrap();

        assert_eq!(
            unpack(&groups),
            expected,
            "changed-voxel seeding missed detached dependents"
        );
    }

    for group in components(&before) {
        let cut: Vec<_> = group
            .intersection(&removed)
            .map(|&p| IVec3::from_array(p))
            .collect();

        if cut.is_empty() {
            continue;
        }

        let cells: Cells = group
            .difference(&removed)
            .map(|p| (*p, before[p]))
            .collect();

        let source = world(&cells, floor);

        let expected = components(&cells);

        for _ in 0..2 {
            let split =
                pollster::block_on(cache.split(&source, &cut, |key| source.leaf(key))).unwrap();

            match split {
                Some(groups) => assert_eq!(unpack(&groups), expected),
                None => assert_eq!(expected.len(), 1, "split early exit lost a fragment"),
            }
        }
    }

    for (&p, &voxel) in &before {
        assert_eq!(original.resident_voxel(IVec3::from_array(p)), voxel);
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(48))]

    #[test]
    fn cached_connectivity_matches_voxel_flood_fill(
        samples in prop::collection::btree_map((-10i32..11, 0i32..11, -2i32..3), (1u64..4, any::<bool>()), 1..160),
        floor in prop::sample::select(vec![-9, 0, 7]),
    ) {
        let mut cells = Cells::new();
        let mut removed = BTreeSet::new();
        for ((x, y, z), (owner, cut)) in samples {
            let p = [x, y + floor, z];
            cells.insert(p, Voxel::new(MaterialId(1), OwnerId(owner)));
            if cut { removed.insert(p); }
        }
        for x in -10..=10 {
            cells.insert([x, floor + 8, 0], Voxel::new(MaterialId(1), OwnerId(if x % 2 == 0 { 1 } else { 2 })));
        }
        for y in 0..8 {
            cells.insert([-10, floor + y, 0], Voxel::new(MaterialId(1), OwnerId(3)));
        }
        for y in 0..10 {
            cells.insert([12, floor + y, 0], Voxel::new(MaterialId(1), OwnerId(if y < 8 { 1 } else { 3 })));
        }
        cells.insert([13, floor + 9, 0], Voxel::new(MaterialId(1), OwnerId(1)));
        removed.insert([-10, floor, 0]);
        removed.insert([0, floor + 8, 0]);
        verify(cells, removed, floor);
    }
}

#[test]
fn local_proof_accepts_interior_edits_but_rejects_lost_anchors_and_bonds() {
    let voxel = Voxel::new(MaterialId(1), OwnerId(1));

    let mut cells: Cells = (0..16)
        .flat_map(|x| (0..3).flat_map(move |y| (0..3).map(move |z| ([x, y, z], voxel))))
        .collect();

    cells.extend((0..8).map(|y| ([-1, y, 0], voxel)));

    for (removed, accepted) in [
        (BTreeSet::from([[3, 1, 1]]), true),
        (
            (0..3)
                .flat_map(|y| (0..3).map(move |z| [7, y, z]))
                .collect(),
            false,
        ),
        (cells.keys().filter(|p| p[1] == 0).copied().collect(), false),
    ] {
        let original = world(&cells, 0);

        let after = world(
            &cells
                .iter()
                .filter(|(p, _)| !removed.contains(*p))
                .map(|(&p, &v)| (p, v))
                .collect(),
            0,
        );

        let changes = removed
            .iter()
            .map(|&p| {
                let key = address(IVec3::from_array(p)).0;

                (key, after.leaf(key).cloned())
            })
            .collect();

        let mut cache = Connectivity::default();

        for _ in 0..65 {
            assert_eq!(
                pollster::block_on(cache.preserves(&original, &changes, |key| original.leaf(key)))
                    .unwrap(),
                accepted
            );
        }

        verify(cells.clone(), removed, 0);
    }
}
