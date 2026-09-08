use engine::{
    ApplicationResult as Result, IVec3,
    scene::{Scene, SceneSettings},
    world::{Material, MaterialId, Owner, OwnerId, StructureId, Voxel, VoxelBounds, WorldSettings},
};

pub fn create() -> Result<Scene> {
    let mut scene = Scene::new(SceneSettings {
        world: WorldSettings {
            bounds: VoxelBounds {
                min: IVec3::new(-160, -3, -160),
                max: IVec3::new(160, 128, 160),
            },
            materials: vec![
                Material {
                    id: MaterialId(1),
                    color: [0.29, 0.43, 0.24],
                    density: 1600.0,
                },
                Material {
                    id: MaterialId(2),
                    color: [0.78, 0.31, 0.13],
                    density: 1800.0,
                },
            ],
            owners: vec![Owner {
                id: OwnerId(1),
                structure: StructureId(1),
            }],
            max_leaves: 2048,
            max_edit_voxels: 1_000_000,
            max_support_voxels: 1_000_000,
        },
        visuals: Default::default(),
        simulation: Default::default(),
        render_leaf_edge: 16,
        max_mesh_vertices: 1_000_000,
        max_bodies: 128,
    })?;

    let mut transaction = scene.transaction();

    for (min, max, material) in [
        ([-100, -3, -100], [100, 0, 100], MaterialId(1)),
        ([-40, 0, -6], [40, 40, 0], MaterialId(2)),
    ] {
        for voxel in (VoxelBounds {
            min: IVec3::from_array(min),
            max: IVec3::from_array(max),
        })
        .iter()
        {
            transaction.place(voxel, Voxel::new(material, OwnerId(1)))?;
        }
    }

    scene.commit(transaction)?;

    Ok(scene)
}
