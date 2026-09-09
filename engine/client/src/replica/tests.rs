#![allow(clippy::unwrap_used)]
use super::*;
use crate::replica::world::{
    Material, MaterialId, Owner, OwnerId, StructureId, VOXEL_SIZE, Voxel, VoxelBounds, address,
    coordinate,
};
use capy_engine_protocol::message::{Chunk, WorldDelta, WorldSnapshot};
use glam::{IVec3, Vec3};
use proptest::prelude::*;
use std::{
    collections::{BTreeMap, BTreeSet},
    time::{Duration, Instant},
};

type Faces = BTreeMap<([i32; 3], [i32; 3], [u32; 3]), usize>;

fn chunks(samples: impl IntoIterator<Item = (IVec3, Voxel)>) -> Vec<Chunk> {
    let mut leaves = BTreeMap::new();

    for (p, v) in samples {
        let (key, index) = address(p);

        leaves.entry(key).or_insert_with(|| vec![Voxel::EMPTY; 512])[index] = v;
    }

    leaves
        .into_iter()
        .map(|(coordinate, values)| {
            let mut palette = Vec::new();

            let mut indices = Vec::new();

            for v in values {
                let i = palette.iter().position(|p| *p == v).unwrap_or_else(|| {
                    palette.push(v);

                    palette.len() - 1
                });

                indices.push(i as u16);
            }

            Chunk {
                coordinate,
                palette,
                indices,
            }
        })
        .collect()
}

fn fixture(edge: i32, samples: impl IntoIterator<Item = (IVec3, Voxel)>) -> Replica {
    Replica::new(
        ReplicaConfig {
            render_leaf_edge: edge,
            material_colors: (1..3)
                .map(|id| (MaterialId(id), [id as f32 / 3.0; 3]))
                .collect(),
            ..Default::default()
        },
        0,
        WorldSnapshot {
            bounds: VoxelBounds {
                min: IVec3::splat(-128),
                max: IVec3::splat(128),
            },
            materials: (1..3)
                .map(|id| Material {
                    id: MaterialId(id),

                    density: 1.0,
                })
                .collect(),
            owners: (1..4)
                .map(|id| Owner {
                    id: OwnerId(id),
                    structure: StructureId(1),
                })
                .collect(),
            chunks: chunks(samples),
            bodies: Vec::new(),
        },
    )
    .unwrap()
}

fn warm(scene: &Replica) {
    let deadline = Instant::now() + Duration::from_secs(10);

    while scene.meshes.values().any(|s| s.ready().is_none()) {
        scene.render_geometry().unwrap();

        assert!(Instant::now() < deadline);

        std::thread::yield_now();
    }
}

fn expected_faces(scene: &Replica) -> Faces {
    let mut faces = BTreeMap::new();

    let group: BTreeMap<_, _> = scene
        .world
        .leaves
        .iter()
        .flat_map(|(&key, leaf)| {
            (0..512).filter_map(move |i| {
                let v = leaf.voxel(i);

                (!v.is_empty()).then_some((coordinate(key, i).to_array(), v))
            })
        })
        .collect();

    for (&p, v) in &group {
        let p = IVec3::from_array(p);

        for normal in [
            IVec3::X,
            -IVec3::X,
            IVec3::Y,
            -IVec3::Y,
            IVec3::Z,
            -IVec3::Z,
        ] {
            if !group.contains_key(&(p + normal).to_array()) {
                faces.insert(
                    (
                        p.to_array(),
                        normal.to_array(),
                        scene.world.material(v.material).unwrap().map(f32::to_bits),
                    ),
                    1,
                );
            }
        }
    }

    faces
}

fn mesh_faces(scene: &Replica) -> Faces {
    let mut faces = BTreeMap::new();

    warm(scene);

    for instance in scene.render_geometry().unwrap().iter() {
        let mesh = &instance.mesh;

        assert_eq!(mesh.vertices.len() % 6, 0);

        for quad in mesh.vertices.chunks_exact(6) {
            let normal = Vec3::from_array(quad[0].normal);

            assert_eq!(normal.abs().element_sum(), 1.0);

            let axis = (0..3).find(|&axis| normal[axis] != 0.0).unwrap();

            let u = (axis + 1) % 3;

            let v = (axis + 2) % 3;

            let mut min = IVec3::splat(i32::MAX);

            let mut max = IVec3::splat(i32::MIN);

            let mut triangles = Vec::new();

            let mut doubled_area = 0.0;

            for triangle in quad.chunks_exact(3) {
                let positions: Vec<_> = triangle
                    .iter()
                    .map(|vertex| {
                        assert_eq!(vertex.normal, quad[0].normal);

                        assert_eq!(vertex.color, quad[0].color);

                        let local = Vec3::from_array(vertex.position);

                        assert!(local.cmpge(mesh.min).all() && local.cmple(mesh.max).all());

                        let p = (local + instance.world_origin) / VOXEL_SIZE;

                        assert!((p - p.round()).abs().max_element() < 1e-3);

                        let p = p.round().as_ivec3();

                        min = min.min(p);
                        max = max.max(p);

                        p
                    })
                    .collect();

                let area = (positions[1] - positions[0])
                    .as_vec3()
                    .cross((positions[2] - positions[0]).as_vec3())
                    .dot(normal);

                assert!(area > 0.0);

                doubled_area += area;

                triangles.push(
                    positions
                        .iter()
                        .map(|p| p.to_array())
                        .collect::<BTreeSet<_>>(),
                );
            }

            assert_eq!(min[axis], max[axis]);

            assert_eq!(
                doubled_area,
                (2 * (max[u] - min[u]) * (max[v] - min[v])) as f32
            );

            assert_ne!(triangles[0], triangles[1]);

            let shared: Vec<_> = triangles[0].intersection(&triangles[1]).collect();

            assert_eq!(shared.len(), 2);

            assert_eq!((shared[0][u] - shared[1][u]).abs(), max[u] - min[u]);

            assert_eq!((shared[0][v] - shared[1][v]).abs(), max[v] - min[v]);

            for a in min[u]..max[u] {
                for b in min[v]..max[v] {
                    let mut p = min;

                    p[u] = a;
                    p[v] = b;
                    p[axis] -= i32::from(normal[axis] > 0.0);
                    *faces
                        .entry((
                            p.to_array(),
                            normal.as_ivec3().to_array(),
                            quad[0].color.map(f32::to_bits),
                        ))
                        .or_default() += 1;
                }
            }
        }
    }

    faces
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(32))]

    // Builds many small arrangements of blocks and checks their visible surfaces against
    // a simple block-by-block count. Every exposed surface must appear exactly once, face
    // outward, and have the right color; hidden surfaces must not be drawn.
    #[test]
    fn greedy_mesh_matches_independent_visible_face_oracle(
        samples in prop::collection::btree_map((-2i32..3, -2i32..3, -2i32..3), (1u16..3, 1u64..4), 1..80),
        edge in prop::sample::select(vec![8, 16, 32]),
    ) {
        let scene = fixture(edge, samples.into_iter().map(|((x,y,z),(material,owner))| (IVec3::new(x,y,z), Voxel::new(MaterialId(material), OwnerId(owner)))));
        assert_eq!(mesh_faces(&scene), expected_faces(&scene));
    }
}

#[test]
fn cached_meshes_follow_boundary_edits_and_enforce_resident_budget() {
    for edge in [8, 16, 32] {
        for boundary in [-edge, 0, edge] {
            let left = IVec3::new(boundary - 1, 0, 0);

            let right = IVec3::new(boundary, 0, 0);

            let a = Voxel::new(MaterialId(1), OwnerId(1));

            let b = Voxel::new(MaterialId(2), OwnerId(2));

            let mut scene = fixture(edge, [(left, a), (right, b)]);

            assert_eq!(mesh_faces(&scene), expected_faces(&scene));

            let old = scene.world.clone();

            let changed = chunks([(left, a)]);

            scene
                .apply(
                    0,
                    1,
                    WorldDelta {
                        chunks: changed,
                        removed_chunks: vec![address(right).0]
                            .into_iter()
                            .filter(|key| *key != address(left).0)
                            .collect(),
                        bodies: Vec::new(),
                        removed_bodies: Vec::new(),
                    },
                )
                .unwrap();

            let fresh = fixture(edge, [(left, a)]);

            assert_eq!(mesh_faces(&scene), mesh_faces(&fresh));

            assert_eq!(old.leaves[&address(right).0].voxel(address(right).1), b);

            scene.settings.max_mesh_vertices = 35;

            assert!(matches!(
                scene.render_geometry(),
                Err(super::ReplicaError::Limit("render vertices"))
            ));

            scene.settings.max_mesh_vertices = 36;

            assert_eq!(mesh_faces(&scene).len(), 6);

            scene.settings.max_mesh_vertices = 1_000_000;

            scene
                .apply(
                    1,
                    2,
                    WorldDelta {
                        chunks: chunks([(left, a), (right, b)]),
                        removed_chunks: Vec::new(),
                        bodies: Vec::new(),
                        removed_bodies: Vec::new(),
                    },
                )
                .unwrap();

            assert_eq!(mesh_faces(&scene), expected_faces(&scene));
        }
    }
}

#[test]
fn shared_model_leaves_match_independent_voxel_faces() {
    let size = IVec3::new(35, 19, 21);

    let sample = |p: IVec3| {
        if p.x == 0 || p.z == size.z - 1 || p.y == 0 || (p.x % 9 == 0 && p.z % 7 == 0) {
            1 + (p.y % 2) as u16
        } else {
            0
        }
    };

    let mut voxels = Vec::new();

    for z in 0..size.z {
        for y in 0..size.y {
            for x in 0..size.x {
                let p = IVec3::new(x, y, z);

                let material = sample(p);

                if material != 0 {
                    voxels.push((p, Voxel::new(MaterialId(material), OwnerId(1))));
                }
            }
        }
    }

    let expected = expected_faces(&fixture(16, voxels));

    for (leaf_edge, ambient_occlusion) in [8, 16, 32, 64]
        .into_iter()
        .flat_map(|edge| [true, false].map(|ao| (edge, ao)))
    {
        let model = VoxelModel::build(
            size,
            &[[1.0 / 3.0; 3], [2.0 / 3.0; 3]],
            |p| (sample(p) != 0).then(|| sample(p) as usize - 1),
            ModelConfig {
                bake_ambient_occlusion: ambient_occlusion,
                leaf_edge,
                ..Default::default()
            },
        )
        .unwrap();

        let mut scene = fixture(16, []);

        scene
            .replace_render_instances(&[VoxelInstance {
                id: 1,
                model,
                translation: Vec3::ZERO,
                rotation: glam::Quat::IDENTITY,
                scale: 1.0,
            }])
            .unwrap();

        assert_eq!(
            mesh_faces(&scene),
            expected,
            "face mismatch for leaf edge {leaf_edge}"
        );
    }
}

#[test]
fn render_instance_replacement_is_atomic_and_does_not_add_raycast_targets() {
    let model = VoxelModel::build(
        IVec3::splat(3),
        &[[0.5; 3]],
        |_| Some(0),
        ModelConfig::default(),
    )
    .unwrap();

    let mut first = VoxelInstance::new(7, model.clone());

    first.translation = Vec3::new(4.0, 0.0, 0.0);

    let mut second = VoxelInstance::new(8, model);

    second.translation = Vec3::new(6.0, 0.0, 0.0);

    let mut scene = Replica::empty(ReplicaConfig::default()).unwrap();

    scene
        .replace_render_instances(&[first, second.clone()])
        .unwrap();

    assert_eq!(scene.render_geometry().unwrap().len(), 2);

    assert!(
        scene
            .raycast(Vec3::new(3.0, 0.1, 0.1), Vec3::X, 10.0)
            .unwrap()
            .is_none()
    );

    scene
        .replace_render_instances(std::slice::from_ref(&second))
        .unwrap();

    let geometry = scene.render_geometry().unwrap();

    assert_eq!(geometry.len(), 1);

    assert!(matches!(geometry[0].id, MeshId::Model(8, _)));

    assert!(matches!(
        scene.replace_render_instances(&[second.clone(), second.clone()]),
        Err(ReplicaError::DuplicateRenderInstance(8))
    ));

    for field in ["translation", "rotation", "scale"] {
        let mut invalid = second.clone();

        match field {
            "translation" => invalid.translation.x = f32::NAN,
            "rotation" => invalid.rotation = glam::Quat::from_xyzw(0.0, 0.0, 0.0, 0.0),
            _ => invalid.scale = 0.0,
        }

        assert!(
            matches!(scene.replace_render_instances(&[invalid]), Err(ReplicaError::InvalidRenderInstance { id: 8, field: invalid_field }) if invalid_field.starts_with(field))
        );

        assert!(std::sync::Arc::ptr_eq(
            &scene.render_geometry().unwrap(),
            &geometry
        ));
    }

    scene.replace_render_instances(&[]).unwrap();

    assert!(scene.render_geometry().unwrap().is_empty());
}
