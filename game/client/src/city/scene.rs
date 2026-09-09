use super::CityConfig;
use engine::{
    IVec3, Quat, Vec3,
    camera::Camera,
    replica::{ModelConfig, Replica, ReplicaConfig, VoxelInstance, VoxelModel},
};
use std::sync::Arc;

pub struct City {
    pub scene: Replica,
    pub camera: Camera,
    pub instances: Vec<VoxelInstance>,
    pub(crate) assets: Vec<Asset>,
    pub(crate) asset_indices: Vec<usize>,
}

pub(crate) struct Asset {
    pub(crate) size: IVec3,
    pub(crate) ground: bool,
}

impl Asset {
    pub(crate) fn sample(&self, p: IVec3) -> Option<usize> {
        if self.ground {
            return Some(3);
        }

        let wall = p.x < 2 || p.z < 2 || p.x >= self.size.x - 2 || p.z >= self.size.z - 2;

        let floor = p.y % 16 < 2 || p.y >= self.size.y - 2;

        let window = (4..12).contains(&(p.y % 16))
            && if p.x < 2 || p.x >= self.size.x - 2 {
                (3..9).contains(&(p.z % 12))
            } else {
                (3..9).contains(&(p.x % 12))
            };

        let door = p.z < 2 && (48..60).contains(&p.x) && p.y < 12;

        if wall && !window && !door {
            Some(0)
        } else if floor {
            Some(2)
        } else if p.x % 24 < 2 && p.z % 24 < 2 {
            Some(1)
        } else {
            None
        }
    }
}

impl City {
    pub fn new(config: CityConfig) -> anyhow::Result<Self> {
        anyhow::ensure!(
            (1..=31).contains(&config.blocks) && config.blocks % 2 == 1,
            "city blocks must be an odd number from 1 to 31"
        );

        let mut assets: Vec<_> = [6, 10, 16, 24]
            .into_iter()
            .map(|floors| Asset {
                size: IVec3::new(112, floors * 16, 96),
                ground: false,
            })
            .collect();

        assets.push(Asset {
            size: IVec3::new(320, 2, 320),
            ground: true,
        });

        let models: Vec<Arc<VoxelModel>> = assets
            .iter()
            .map(|asset| {
                VoxelModel::build(
                    asset.size,
                    &[
                        [0.63, 0.66, 0.69],
                        [0.51, 0.25, 0.17],
                        [0.55, 0.52, 0.46],
                        [0.18, 0.19, 0.21],
                    ],
                    |p| asset.sample(p),
                    ModelConfig {
                        bake_ambient_occlusion: config.visuals.ambient_occlusion,
                        leaf_edge: config.leaf_edge,
                        max_vertices: 2_000_000,
                    },
                )
            })
            .collect::<Result<_, _>>()?;

        let mut instances = Vec::new();

        let mut asset_indices = Vec::new();

        let half = config.blocks / 2;

        for z in -half..=half {
            for x in -half..=half {
                let archetype = (x * 13 + z * 7).rem_euclid(4) as usize;

                for (index, offset, scale) in [
                    (4, Vec3::new(0.0, -0.2, 0.0), 1.0),
                    (archetype, Vec3::new(5.0, 0.0, 6.0), 2.0),
                ] {
                    instances.push(VoxelInstance {
                        translation: Vec3::new(x as f32 * 32.0, 0.0, z as f32 * 32.0) + offset,
                        scale,
                        ..VoxelInstance::new(instances.len() as u64 + 1, models[index].clone())
                    });

                    asset_indices.push(index);
                }
            }
        }

        let mut scene = Replica::empty(ReplicaConfig {
            max_mesh_vertices: 8_000_000,
            visuals: config.visuals,
            ..Default::default()
        })?;

        scene.replace_render_instances(&instances)?;

        Ok(Self {
            scene,
            camera: Camera {
                position: Vec3::new(0.0, 8.0, -100.0),
                direction: Vec3::new(0.15, 0.12, 1.0).normalize(),
                fov_radians: 70.0_f32.to_radians(),
                near_plane: 0.05,
            },
            instances,
            assets,
            asset_indices,
        })
    }

    pub fn animate(&mut self, frame: u32) -> anyhow::Result<()> {
        if let Some(instance) = self.instances.get_mut(1) {
            instance.rotation = Quat::from_rotation_y((frame as f32 * 0.01).sin() * 0.2);

            self.scene.replace_render_instances(&self.instances)?;
        }

        Ok(())
    }
}
