#![allow(clippy::unwrap_used)]

use super::*;
use glam::IVec3;
use proptest::prelude::*;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

pub(crate) fn settings() -> WorldSettings {
    WorldSettings {
        bounds: VoxelBounds {
            min: IVec3::splat(-640),
            max: IVec3::splat(640),
        },
        materials: (1..=2)
            .map(|id| Material {
                id: MaterialId(id),

                density: id as f32 * 1000.0,
            })
            .collect(),
        owners: (1..=3)
            .map(|id| Owner {
                id: OwnerId(id),
                structure: StructureId(id),
            })
            .collect(),
        max_leaves: 4096,
        max_edit_voxels: 16384,
        max_support_voxels: 16384,
    }
}

pub(crate) fn voxel(owner: u64) -> Voxel {
    Voxel::new(MaterialId(1), OwnerId(owner))
}

fn coordinate_strategy() -> impl Strategy<Value = [i32; 3]> {
    let axis = prop_oneof![
        3 => prop::sample::select(vec![-321, -320, -319, -9, -8, -7, -1, 0, 1, 7, 8, 9, 319, 320, 321]),
        1 => -330i32..330,
    ];

    (axis.clone(), axis.clone(), axis).prop_map(|(x, y, z)| [x, y, z])
}

fn assert_model(
    world: &WorldRead,
    model: &BTreeMap<[i32; 3], Voxel>,
    touched: &BTreeSet<[i32; 3]>,
) {
    for &p in touched {
        assert_eq!(
            world.voxel(IVec3::from_array(p)).unwrap(),
            model.get(&p).copied().unwrap_or(Voxel::EMPTY),
            "coordinate {p:?}"
        );
    }

    for owner in 1..=3 {
        let expected: BTreeMap<_, _> = model
            .iter()
            .filter(|(_, v)| v.owner == OwnerId(owner))
            .map(|(&p, &v)| (p, v))
            .collect();

        let actual: BTreeMap<_, _> = world
            .owner_voxels(OwnerId(owner))
            .map(|(p, v)| (p.to_array(), v))
            .collect();

        assert_eq!(actual, expected, "owner {owner}");
    }

    let leaves: BTreeSet<_> = model
        .keys()
        .map(|p| p.map(|v| (v as f64 / 8.0).floor() as i32))
        .collect();

    assert_eq!(world.leaf_count(), leaves.len());
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]

    // Tries many different sequences of adding, changing, and removing blocks, checking that
    // the world always contains the expected blocks and that saved earlier versions stay unchanged.
    // Outdated edits are rejected, and removing every block leaves no occupied storage behind.
    #[test]
    fn transaction_sequences_match_reference_world(
        operations in prop::collection::vec((coordinate_strategy(), 0u8..5, 1u64..4, 1u16..3), 1..100)
    ) {
        let mut world = WorldRead::new(settings()).unwrap();
        let mut model = BTreeMap::new();
        let mut touched = BTreeSet::new();
        let mut snapshots = Vec::new();
        for (index, (p, operation, owner, material)) in operations.into_iter().enumerate() {
            touched.insert(p);
            let coordinate = IVec3::from_array(p);
            let previous = model.get(&p).copied().unwrap_or(Voxel::EMPTY);
            let value = match operation {
                0 => Voxel::EMPTY,
                1 => previous,
                _ => Voxel::new(MaterialId(material), OwnerId(owner)),
            };
            let epoch = world.epoch();
            let stale = Transaction::new(&world);
            let mut transaction = Transaction::new(&world);
            if operation == 0 {
                transaction.remove(coordinate).unwrap();
            } else if previous.is_empty() {
                transaction.place(coordinate, value).unwrap();
            } else {
                transaction.replace(coordinate, previous, value).unwrap();
            }
            let (next, changed) = prepare(&world, transaction).unwrap();
            assert_eq!(changed, if previous == value { vec![] } else { vec![coordinate] }, "operation {index}");
            assert_eq!(next.epoch(), epoch + u64::from(previous != value));
            if previous != value {
                assert!(matches!(prepare(&next, stale), Err(WorldError::Stale)));
            } else {
                assert!(prepare(&next, stale).is_ok());
            }
            world = next;
            if value.is_empty() {
                model.remove(&p);
            } else {
                model.insert(p, value);
            }
            assert_model(&world, &model, &touched);
            for (snapshot, expected) in &snapshots {
                assert_model(snapshot, expected, &touched);
            }
            if index % 10 == 0 {
                snapshots.push((world.clone(), model.clone()));
            }
        }
        let snapshot = world.clone();
        let mut transaction = Transaction::new(&world);
        for &p in model.keys() {
            transaction.remove(IVec3::from_array(p)).unwrap();
        }
        let (empty, _) = prepare(&world, transaction).unwrap();
        assert_model(&empty, &BTreeMap::new(), &touched);
        assert_model(&snapshot, &model, &touched);
    }
}

// Checks that one invalid change causes the entire batch of edits to be rejected,
// even if earlier changes were valid. Edits made for a different world are rejected too.
#[test]
fn poisoned_transactions_cannot_publish_earlier_valid_edits() {
    for failure in [
        "duplicate",
        "conflict",
        "invalid",
        "bounds",
        "edit limit",
        "leaf limit",
        "foreign",
    ] {
        let mut config = settings();

        config.max_edit_voxels = if failure == "edit limit" { 1 } else { 10 };
        config.max_leaves = 1;

        let world = WorldRead::new(config).unwrap();

        let mut transaction = Transaction::new(&world);

        transaction.place(IVec3::ZERO, voxel(1)).unwrap();

        match failure {
            "duplicate" => assert!(matches!(
                transaction.place(IVec3::ZERO, voxel(1)),
                Err(WorldError::Duplicate(_))
            )),
            "conflict" => assert!(matches!(
                transaction.replace(IVec3::X, voxel(1), voxel(2)),
                Err(WorldError::Conflict(_))
            )),
            "invalid" => assert!(matches!(
                transaction.place(IVec3::X, voxel(99)),
                Err(WorldError::Invalid)
            )),
            "bounds" => assert!(matches!(
                transaction.remove(world.bounds().max),
                Err(WorldError::Unavailable(_))
            )),
            "edit limit" => assert!(matches!(
                transaction.place(IVec3::X, voxel(1)),
                Err(WorldError::Limit("transaction voxels"))
            )),
            "leaf limit" => transaction.place(IVec3::splat(8), voxel(1)).unwrap(),
            "foreign" => {
                let other = WorldRead::new(settings()).unwrap();

                assert!(matches!(
                    prepare(&other, transaction),
                    Err(WorldError::Stale)
                ));

                continue;
            }
            _ => unreachable!(),
        }

        if !matches!(failure, "leaf limit") {
            assert!(transaction.place(IVec3::Y, voxel(1)).is_err(), "{failure}");
        }

        assert!(prepare(&world, transaction).is_err(), "{failure}");

        assert_eq!(world.epoch(), 0, "{failure}");

        assert_eq!(world.leaf_count(), 0, "{failure}");

        assert_eq!(world.voxel(IVec3::ZERO).unwrap(), Voxel::EMPTY, "{failure}");
    }
}

// Fills the same area with identical blocks, a repeating mix, and many different blocks,
// then empties it. Checks that changing how the blocks are stored never loses information
// or changes any saved earlier version of the world.
#[test]
fn leaf_representations_preserve_contents_and_old_snapshots() {
    let mut config = settings();

    config.owners = (1..=512)
        .map(|id| Owner {
            id: OwnerId(id),
            structure: StructureId(id),
        })
        .collect();

    let mut world = WorldRead::new(config).unwrap();

    let origin = IVec3::splat(-8);

    let mut snapshots = Vec::new();

    for phase in 0..4 {
        let mut transaction = Transaction::new(&world);

        let expected: Vec<_> = (0..512)
            .map(|i| match phase {
                0 => voxel(1),
                1 => voxel(1 + i as u64 % 2),
                2 => voxel(1 + i as u64),
                _ => Voxel::EMPTY,
            })
            .collect();

        for (i, &value) in expected.iter().enumerate() {
            let p = origin + IVec3::new((i % 8) as i32, (i / 8 % 8) as i32, (i / 64) as i32);

            transaction
                .replace(p, world.voxel(p).unwrap(), value)
                .unwrap();
        }

        world = prepare(&world, transaction).unwrap().0;

        match phase {
            0 => assert!(matches!(
                world.leaf([-1; 3]).map(Arc::as_ref),
                Some(Leaf::Uniform(_))
            )),
            1 => assert!(matches!(
                world.leaf([-1; 3]).map(Arc::as_ref),
                Some(Leaf::Palette { .. })
            )),
            2 => assert!(matches!(
                world.leaf([-1; 3]).map(Arc::as_ref),
                Some(Leaf::Dense(_))
            )),
            _ => assert_eq!(world.leaf_count(), 0),
        }

        snapshots.push((world.clone(), expected));

        for (snapshot, expected) in &snapshots {
            for (i, &value) in expected.iter().enumerate() {
                let p = origin + IVec3::new((i % 8) as i32, (i / 8 % 8) as i32, (i / 64) as i32);

                assert_eq!(
                    snapshot.voxel(p).unwrap(),
                    value,
                    "phase {phase}, index {i}"
                );
            }
        }
    }
}
