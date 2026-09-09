use capy_engine_server::{
    IVec3,
    simulation::{Simulation, SimulationConfig},
    world::{Material, MaterialId, Owner, OwnerId, StructureId, Voxel, VoxelBounds, WorldSettings},
};

pub fn create() -> anyhow::Result<Simulation> {
    let mut scene = Simulation::new(SimulationConfig {
        world: WorldSettings {
            bounds: VoxelBounds {
                min: IVec3::new(-160, -3, -160),
                max: IVec3::new(160, 128, 160),
            },
            materials: vec![
                Material {
                    id: MaterialId(1),

                    density: 1600.0,
                },
                Material {
                    id: MaterialId(2),

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
        physics: Default::default(),
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
