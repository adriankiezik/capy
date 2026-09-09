#![allow(clippy::unwrap_used)]
use super::{ModelConfig, ModelError, VoxelInstance, VoxelModel};
use crate::{IVec3, Quat, VOXEL_SIZE, Vec3};

#[test]
fn palette_indices_bounds_and_identity_defaults_are_explicit() {
    let dimensions = IVec3::new(3, 4, 5);

    let empty = VoxelModel::build(dimensions, &[], |_| None, ModelConfig::default()).unwrap();

    assert_eq!(empty.vertex_count(), 0);

    assert_eq!(empty.voxel_dimensions(), dimensions);

    assert_eq!(empty.local_bounds().min, Vec3::ZERO);

    assert_eq!(empty.local_bounds().max, dimensions.as_vec3() * VOXEL_SIZE);

    let solid = VoxelModel::build(
        dimensions,
        &[[0.1, 0.2, 0.3]],
        |_| Some(0),
        ModelConfig::default(),
    )
    .unwrap();

    assert!(solid.vertex_count() > 0);

    assert!(
        solid
            .leaves
            .iter()
            .flat_map(|(_, _, mesh)| &mesh.vertices)
            .all(|vertex| vertex.color == [0.1, 0.2, 0.3])
    );

    let instance = VoxelInstance::new(42, solid);

    assert_eq!(instance.id, 42);

    assert_eq!(instance.translation, Vec3::ZERO);

    assert_eq!(instance.rotation, Quat::IDENTITY);

    assert_eq!(instance.scale, 1.0);
}

#[test]
fn invalid_palette_references_are_rejected_even_inside_solid_models() {
    let result = VoxelModel::build(
        IVec3::splat(3),
        &[[0.5; 3]],
        |p| Some(usize::from(p == IVec3::ONE)),
        ModelConfig::default(),
    );

    assert!(matches!(
        result,
        Err(ModelError::UndefinedMaterial {
            position: IVec3::ONE,
            index: 1,
            palette_len: 1
        })
    ));

    assert!(matches!(
        VoxelModel::build(IVec3::ONE, &[], |_| Some(0), ModelConfig::default()),
        Err(ModelError::UndefinedMaterial {
            position: IVec3::ZERO,
            index: 0,
            palette_len: 0
        })
    ));
}

#[test]
fn model_errors_identify_invalid_build_inputs() {
    assert!(matches!(
        VoxelModel::build(IVec3::ZERO, &[], |_| None, ModelConfig::default()),
        Err(ModelError::InvalidDimensions(IVec3::ZERO))
    ));

    assert!(matches!(
        VoxelModel::build(
            IVec3::ONE,
            &[],
            |_| None,
            ModelConfig {
                leaf_edge: 7,
                ..Default::default()
            }
        ),
        Err(ModelError::InvalidLeafEdge(7))
    ));

    assert!(matches!(
        VoxelModel::build(
            IVec3::ONE,
            &[],
            |_| None,
            ModelConfig {
                max_vertices: 0,
                ..Default::default()
            }
        ),
        Err(ModelError::EmptyVertexBudget)
    ));

    assert!(matches!(
        VoxelModel::build(
            IVec3::ONE,
            &[[0.0, f32::NAN, 0.0]],
            |_| Some(0),
            ModelConfig::default()
        ),
        Err(ModelError::InvalidColor {
            index: 0,
            channel: 1,
            ..
        })
    ));
}
