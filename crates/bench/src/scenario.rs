use anyhow::Result;
use clap::ValueEnum;
use engine::{
    IVec3, Vec2, Vec3, View,
    player::{Player, PlayerInput, PlayerPose, PlayerSettings},
    scene::{Hit, Scene, SceneSettings},
    ui::{Canvas, Rect, TextStyle},
    world::{Material, MaterialId, Owner, OwnerId, StructureId, Voxel, VoxelBounds, WorldSettings},
};
use serde::Serialize;
use std::time::Duration;

pub const STEP: Duration = Duration::from_nanos(16_666_667);

#[derive(Clone, Copy, ValueEnum, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Kind {
    TownWalk,
    Destruction,
    UiHeavy,
}

impl Kind {
    pub fn name(self) -> &'static str {
        match self {
            Self::TownWalk => "town-walk",
            Self::Destruction => "destruction",
            Self::UiHeavy => "ui-heavy",
        }
    }
}

pub struct Scenario {
    scene: Scene,
    player: Player,
    selection: Option<Hit>,
    kind: Kind,
}

fn anchor(index: u32) -> IVec3 {
    IVec3::new(
        (index % 12) as i32 * 12 - 72,
        19,
        (index / 12) as i32 * 12 - 60,
    )
}

impl Scenario {
    pub fn new(kind: Kind) -> Result<Self> {
        let mut scene = Scene::new(SceneSettings {
            world: WorldSettings {
                bounds: VoxelBounds {
                    min: IVec3::new(-160, -1, -160),
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
                owners: vec![
                    Owner {
                        id: OwnerId(1),
                        structure: StructureId(1),
                    },
                    Owner {
                        id: OwnerId(2),
                        structure: StructureId(2),
                    },
                ],
                max_leaves: 4096,
                max_edit_voxels: 1_000_000,
                max_support_voxels: 1_000_000,
            },
            visuals: Default::default(),
            simulation: Default::default(),
            render_leaf_edge: 16,
            max_mesh_vertices: 2_000_000,
            max_bodies: 256,
        })?;

        let mut transaction = scene.transaction();

        for z in -96..96 {
            for x in -96..96 {
                transaction.place(IVec3::new(x, -1, z), Voxel::new(MaterialId(1), OwnerId(1)))?;
            }
        }

        if matches!(kind, Kind::Destruction) {
            for index in 0..120 {
                let anchor = anchor(index);

                for y in 0..20 {
                    transaction.place(
                        IVec3::new(anchor.x, y, anchor.z),
                        Voxel::new(MaterialId(2), OwnerId(1)),
                    )?;
                }

                for x in 0..6 {
                    for z in 0..2 {
                        transaction.place(
                            anchor + IVec3::new(x, 1, z),
                            Voxel::new(MaterialId(2), OwnerId(2)),
                        )?;
                    }
                }
            }
        } else {
            for bz in [-60, -20, 20, 60] {
                for bx in [-60, -20, 20, 60] {
                    for y in 0..24 {
                        for z in 0..12 {
                            for x in 0..12 {
                                if x == 0 || x == 11 || z == 0 || z == 11 || y == 23 {
                                    transaction.place(
                                        IVec3::new(bx + x, y, bz + z),
                                        Voxel::new(MaterialId(2), OwnerId(1)),
                                    )?;
                                }
                            }
                        }
                    }
                }
            }
        }

        scene.commit(transaction)?;

        let player = Player::new(
            PlayerSettings::default(),
            PlayerPose {
                position: Vec3::new(0.8, 0.01, 7.5),
                pitch_radians: -0.1,
                ..Default::default()
            },
        )?;

        Ok(Self {
            scene,
            player,
            selection: None,
            kind,
        })
    }

    pub fn update(&mut self, frame: u32) -> Result<()> {
        if matches!(self.kind, Kind::Destruction) && frame % 30 == 15 {
            let mut transaction = self.scene.transaction();

            transaction.remove(anchor(frame / 30))?;

            self.scene.commit(transaction)?;
        }

        self.scene.advance(STEP)?;

        let cycle = frame % 480;

        self.player
            .rotate_view(if cycle < 240 { 0.003 } else { -0.003 }, 0.0)?;

        self.player.advance(
            &self.scene,
            PlayerInput {
                movement: Vec2::new(0.0, if cycle < 240 { 1.0 } else { -1.0 }),
                sprint: false,
                jump: frame % 180 == 90,
            },
            STEP,
        )?;

        let camera = self.player.camera();

        self.selection = self.scene.raycast(camera.position, camera.direction, 8.0)?;

        Ok(())
    }

    pub fn canvas(&self, frame: u32, size: [u32; 2]) -> Canvas {
        let canvas = Canvas::new();

        let style = TextStyle::default();

        canvas.text(
            format!("Capy benchmark | {} | frame {frame}", self.kind.name()),
            Vec2::splat(16.0),
            &style,
        );

        let center = Vec2::new(size[0] as f32, size[1] as f32) * 0.5;

        canvas.rect(
            Rect::new(center - Vec2::new(4.0, 1.0), Vec2::new(8.0, 2.0)),
            [1.0; 4],
        );

        if matches!(self.kind, Kind::UiHeavy) {
            let panel = canvas
                .panel()
                .size(Vec2::new(size[0] as f32 * 0.65, size[1] as f32 * 0.75))
                .offset(Vec2::new(16.0, 48.0))
                .background([0.08, 0.1, 0.14, 0.9])
                .padding(12.0);

            let grid = panel.row().wrap(true).gap(8.0);

            for index in 0..96 {
                let item = grid
                    .column()
                    .size(Vec2::new(90.0, 54.0))
                    .background([0.2, 0.25, 0.32, 0.8]);

                item.text(format!("Item {index}"), &style);

                item.text(format!("x{}", (frame / 30 + index) % 64), &style);
            }
        }

        canvas
    }

    pub fn view(&self) -> View<'_> {
        View::new(&self.scene, self.player.camera()).with_selection(self.selection)
    }
}
