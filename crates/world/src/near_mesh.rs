use std::collections::{HashMap, HashSet};

use capy_core::{
    BakedChunkData, MaterialId, NearVoxelMeshChunk, NearVoxelMeshData, VoxelSurfaceVertex,
    is_water_material,
};

use crate::tree_patch::{LeafBrickEdit, extract_leaf_bricks};

const BRANCH: i32 = 4;
const BRANCH_USIZE: usize = BRANCH as usize;

type BrickCoord = (i32, i32, i32);
type BrickMap = HashMap<BrickCoord, [MaterialId; 64]>;

#[derive(Clone, Copy)]
struct FaceDirection {
    axis: usize,
    sign: i32,
    u_axis: usize,
    v_axis: usize,
    normal: [f32; 3],
}

const FACE_DIRECTIONS: [FaceDirection; 6] = [
    FaceDirection {
        axis: 0,
        sign: 1,
        u_axis: 1,
        v_axis: 2,
        normal: [1.0, 0.0, 0.0],
    },
    FaceDirection {
        axis: 0,
        sign: -1,
        u_axis: 2,
        v_axis: 1,
        normal: [-1.0, 0.0, 0.0],
    },
    FaceDirection {
        axis: 1,
        sign: 1,
        u_axis: 2,
        v_axis: 0,
        normal: [0.0, 1.0, 0.0],
    },
    FaceDirection {
        axis: 1,
        sign: -1,
        u_axis: 0,
        v_axis: 2,
        normal: [0.0, -1.0, 0.0],
    },
    FaceDirection {
        axis: 2,
        sign: 1,
        u_axis: 0,
        v_axis: 1,
        normal: [0.0, 0.0, 1.0],
    },
    FaceDirection {
        axis: 2,
        sign: -1,
        u_axis: 1,
        v_axis: 0,
        normal: [0.0, 0.0, -1.0],
    },
];

/// Build a deliberately bounded near-field mesh for edited chunks.
///
/// Quads are greedily merged inside each 4³ leaf brick. A chunk that exceeds
/// `max_quads_per_chunk` is omitted completely, so the renderer will continue
/// to ray trace it instead of displaying a partial mesh.
pub fn build_near_voxel_mesh(
    chunks: &HashMap<[i32; 3], BakedChunkData>,
    chunk_size_xz: u32,
    chunk_size_y: u32,
    max_quads_per_chunk: usize,
) -> NearVoxelMeshData {
    let cache =
        build_near_voxel_mesh_cache(chunks, chunk_size_xz, chunk_size_y, max_quads_per_chunk);
    let mut result = NearVoxelMeshData::default();
    let mut coords: Vec<_> = cache.keys().copied().collect();
    coords.sort_unstable();
    for coord in coords {
        append_chunk_mesh(&mut result, &cache[&coord]);
    }

    result
}

pub fn build_near_voxel_mesh_cache(
    chunks: &HashMap<[i32; 3], BakedChunkData>,
    chunk_size_xz: u32,
    chunk_size_y: u32,
    max_quads_per_chunk: usize,
) -> HashMap<[i32; 3], NearVoxelMeshData> {
    chunks
        .iter()
        .filter_map(|(&coord, baked)| {
            build_near_voxel_chunk_mesh(
                baked,
                coord,
                chunk_size_xz,
                chunk_size_y,
                max_quads_per_chunk,
            )
            .map(|mesh| (coord, mesh))
        })
        .collect()
}

/// Build one independently cacheable edited-chunk mesh.
pub fn build_near_voxel_chunk_mesh(
    baked: &BakedChunkData,
    chunk_coord: [i32; 3],
    chunk_size_xz: u32,
    chunk_size_y: u32,
    max_quads_per_chunk: usize,
) -> Option<NearVoxelMeshData> {
    let bricks = extract_leaf_bricks(baked);
    let (vertices, indices) = mesh_chunk(
        &bricks,
        chunk_coord,
        chunk_size_xz,
        chunk_size_y,
        max_quads_per_chunk,
    )?;
    let index_count = indices.len() as u32;
    Some(NearVoxelMeshData {
        vertices,
        indices,
        chunks: vec![NearVoxelMeshChunk {
            coord: chunk_coord,
            index_start: 0,
            index_count,
        }],
        ..Default::default()
    })
}

/// Assemble cached edited meshes and one local-space canonical mesh.
///
/// This avoids rerunning surface extraction for untouched edited chunks. The
/// canonical geometry is appended once and instanced over every unedited slot.
pub fn assemble_hybrid_near_mesh(
    cached_edited: &HashMap<[i32; 3], NearVoxelMeshData>,
    canonical_mesh: &NearVoxelMeshData,
    edited_chunks: &HashMap<[i32; 3], BakedChunkData>,
    grid_dim_xz: u32,
) -> NearVoxelMeshData {
    let mut result = NearVoxelMeshData::default();
    let mut coords: Vec<_> = cached_edited.keys().copied().collect();
    coords.sort_unstable();
    for coord in coords {
        append_chunk_mesh(&mut result, &cached_edited[&coord]);
    }

    let vertex_base = result.vertices.len() as u32;
    result.canonical_index_start = result.indices.len() as u32;
    result.vertices.extend_from_slice(&canonical_mesh.vertices);
    result.indices.extend(
        canonical_mesh
            .indices
            .iter()
            .map(|index| index + vertex_base),
    );
    result.canonical_index_count = result.indices.len() as u32 - result.canonical_index_start;

    let half = (grid_dim_xz / 2) as i32;
    for z in 0..grid_dim_xz {
        for x in 0..grid_dim_xz {
            let coord = [x as i32 - half, 0, z as i32 - half];
            if !edited_chunks.contains_key(&coord) {
                result.canonical_chunks.push(coord);
            }
        }
    }

    result
}

fn append_chunk_mesh(result: &mut NearVoxelMeshData, chunk_mesh: &NearVoxelMeshData) {
    let vertex_base = result.vertices.len() as u32;
    let index_start = result.indices.len() as u32;
    result.vertices.extend_from_slice(&chunk_mesh.vertices);
    result
        .indices
        .extend(chunk_mesh.indices.iter().map(|index| index + vertex_base));
    for chunk in &chunk_mesh.chunks {
        result.chunks.push(NearVoxelMeshChunk {
            coord: chunk.coord,
            index_start: index_start + chunk.index_start,
            index_count: chunk.index_count,
        });
    }
}

fn mesh_chunk(
    bricks: &[LeafBrickEdit],
    chunk_coord: [i32; 3],
    chunk_size_xz: u32,
    chunk_size_y: u32,
    max_quads: usize,
) -> Option<(Vec<VoxelSurfaceVertex>, Vec<u32>)> {
    let brick_map: BrickMap = bricks
        .iter()
        .map(|brick| {
            (
                (brick.bx as i32, brick.by as i32, brick.bz as i32),
                brick.materials,
            )
        })
        .collect();
    let full_opaque_bricks: HashSet<BrickCoord> = brick_map
        .iter()
        .filter_map(|(&coord, materials)| {
            materials
                .iter()
                .all(|&material| material != 0 && !is_water_material(material))
                .then_some(coord)
        })
        .collect();
    let full_water_bricks: HashSet<BrickCoord> = brick_map
        .iter()
        .filter_map(|(&coord, materials)| {
            materials
                .iter()
                .all(|&material| is_water_material(material))
                .then_some(coord)
        })
        .collect();

    let mut vertices = Vec::new();
    let mut indices = Vec::new();
    let world_offset = [
        chunk_coord[0] as f32 * chunk_size_xz as f32,
        chunk_coord[1] as f32 * chunk_size_y as f32,
        chunk_coord[2] as f32 * chunk_size_xz as f32,
    ];

    for (&brick_coord, materials) in &brick_map {
        let full_neighbours_in = |class: &HashSet<BrickCoord>| {
            class.contains(&brick_coord)
                && FACE_DIRECTIONS.iter().all(|direction| {
                    let mut neighbour = [brick_coord.0, brick_coord.1, brick_coord.2];
                    neighbour[direction.axis] += direction.sign;
                    class.contains(&(neighbour[0], neighbour[1], neighbour[2]))
                })
        };
        if full_neighbours_in(&full_opaque_bricks) || full_neighbours_in(&full_water_bricks) {
            continue;
        }

        for direction in FACE_DIRECTIONS {
            for slice in 0..BRANCH {
                let mut mask = [0 as MaterialId; 16];
                for v in 0..BRANCH {
                    for u in 0..BRANCH {
                        let mut local = [0i32; 3];
                        local[direction.axis] = slice;
                        local[direction.u_axis] = u;
                        local[direction.v_axis] = v;
                        let bit = local_index(local);
                        let material = materials[bit];
                        if material == 0 {
                            continue;
                        }

                        let voxel = [
                            brick_coord.0 * BRANCH + local[0],
                            brick_coord.1 * BRANCH + local[1],
                            brick_coord.2 * BRANCH + local[2],
                        ];
                        let mut neighbour = voxel;
                        neighbour[direction.axis] += direction.sign;
                        let neighbour_material = material_at(&brick_map, neighbour);
                        let visible = if is_water_material(material) {
                            !is_water_material(neighbour_material)
                        } else {
                            neighbour_material == 0 || is_water_material(neighbour_material)
                        };
                        if visible {
                            mask[(u + v * BRANCH) as usize] = material;
                        }
                    }
                }

                let mut used = [false; 16];
                for v in 0..BRANCH {
                    for u in 0..BRANCH {
                        let mask_index = (u + v * BRANCH) as usize;
                        let material = mask[mask_index];
                        if material == 0 || used[mask_index] {
                            continue;
                        }

                        let mut width = 1;
                        while u + width < BRANCH {
                            let next = (u + width + v * BRANCH) as usize;
                            if used[next] || mask[next] != material {
                                break;
                            }
                            width += 1;
                        }

                        let mut height = 1;
                        'height: while v + height < BRANCH {
                            for du in 0..width {
                                let next = (u + du + (v + height) * BRANCH) as usize;
                                if used[next] || mask[next] != material {
                                    break 'height;
                                }
                            }
                            height += 1;
                        }

                        for dv in 0..height {
                            for du in 0..width {
                                used[(u + du + (v + dv) * BRANCH) as usize] = true;
                            }
                        }

                        if indices.len() / 6 >= max_quads {
                            return None;
                        }
                        emit_quad(
                            &mut vertices,
                            &mut indices,
                            brick_coord,
                            world_offset,
                            direction,
                            slice,
                            u,
                            v,
                            width,
                            height,
                            material,
                        );
                    }
                }
            }
        }
    }

    Some((vertices, indices))
}

#[allow(clippy::too_many_arguments)]
fn emit_quad(
    vertices: &mut Vec<VoxelSurfaceVertex>,
    indices: &mut Vec<u32>,
    brick_coord: BrickCoord,
    world_offset: [f32; 3],
    direction: FaceDirection,
    slice: i32,
    u: i32,
    v: i32,
    width: i32,
    height: i32,
    material: MaterialId,
) {
    let mut origin = [
        world_offset[0] + (brick_coord.0 * BRANCH) as f32,
        world_offset[1] + (brick_coord.1 * BRANCH) as f32,
        world_offset[2] + (brick_coord.2 * BRANCH) as f32,
    ];
    origin[direction.axis] += (slice + i32::from(direction.sign > 0)) as f32;
    origin[direction.u_axis] += u as f32;
    origin[direction.v_axis] += v as f32;

    let mut du = [0.0f32; 3];
    du[direction.u_axis] = width as f32;
    let mut dv = [0.0f32; 3];
    dv[direction.v_axis] = height as f32;

    let positions = [
        origin,
        add(origin, du),
        add(add(origin, du), dv),
        add(origin, dv),
    ];
    let base = vertices.len() as u32;
    vertices.extend(positions.map(|position| VoxelSurfaceVertex {
        position,
        normal: direction.normal,
        material,
    }));
    indices.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
}

fn material_at(bricks: &BrickMap, voxel: [i32; 3]) -> MaterialId {
    if voxel.iter().any(|&coord| coord < 0) {
        return 0;
    }
    let brick = (
        voxel[0].div_euclid(BRANCH),
        voxel[1].div_euclid(BRANCH),
        voxel[2].div_euclid(BRANCH),
    );
    let Some(materials) = bricks.get(&brick) else {
        return 0;
    };
    let local = [
        voxel[0].rem_euclid(BRANCH),
        voxel[1].rem_euclid(BRANCH),
        voxel[2].rem_euclid(BRANCH),
    ];
    materials[local_index(local)]
}

fn local_index(local: [i32; 3]) -> usize {
    local[0] as usize
        + local[1] as usize * BRANCH_USIZE
        + local[2] as usize * BRANCH_USIZE * BRANCH_USIZE
}

fn add(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::{assemble_hybrid_near_mesh, mesh_chunk};
    use crate::LeafBrickEdit;
    use capy_core::{NearVoxelMeshData, VoxelSurfaceVertex, WATER_BIT, is_water_material};

    #[test]
    fn single_voxel_emits_six_quads() {
        let mut materials = [0; 64];
        materials[0] = 1;
        let bricks = [LeafBrickEdit {
            bx: 0,
            by: 0,
            bz: 0,
            materials,
        }];
        let Some((vertices, indices)) = mesh_chunk(&bricks, [0, 0, 0], 256, 1024, 100) else {
            panic!("small mesh should fit the test budget");
        };
        assert_eq!(vertices.len(), 24);
        assert_eq!(indices.len(), 36);
    }

    #[test]
    fn solid_leaf_is_greedily_reduced_to_six_quads() {
        let bricks = [LeafBrickEdit {
            bx: 0,
            by: 0,
            bz: 0,
            materials: [1; 64],
        }];
        let Some((vertices, indices)) = mesh_chunk(&bricks, [0, 0, 0], 256, 1024, 100) else {
            panic!("small mesh should fit the test budget");
        };
        assert_eq!(vertices.len(), 24);
        assert_eq!(indices.len(), 36);
    }

    #[test]
    fn opaque_water_boundary_is_available_to_both_raster_passes() {
        let mut materials = [0; 64];
        materials[0] = 1;
        materials[1] = 8 | WATER_BIT;
        let bricks = [LeafBrickEdit {
            bx: 0,
            by: 0,
            bz: 0,
            materials,
        }];
        let Some((vertices, indices)) = mesh_chunk(&bricks, [0, 0, 0], 256, 1024, 100) else {
            panic!("small mixed mesh should fit the test budget");
        };

        assert_eq!(indices.len(), 72);
        assert!(
            vertices
                .iter()
                .any(|vertex| is_water_material(vertex.material))
        );
        assert!(
            vertices
                .iter()
                .any(|vertex| vertex.material != 0 && !is_water_material(vertex.material))
        );
    }

    #[test]
    fn canonical_mesh_is_instanced_only_for_unedited_slots() {
        let canonical_mesh = NearVoxelMeshData {
            vertices: vec![VoxelSurfaceVertex {
                position: [0.0; 3],
                normal: [0.0, 1.0, 0.0],
                material: 1,
            }],
            indices: vec![0],
            ..Default::default()
        };
        let edited_coord = [0, 0, 0];
        let mut edited = HashMap::new();
        edited.insert(edited_coord, crate::generate_flat_baked().unwrap());

        let assembled = assemble_hybrid_near_mesh(&HashMap::new(), &canonical_mesh, &edited, 2);

        assert_eq!(assembled.canonical_index_count, 1);
        assert_eq!(assembled.canonical_chunks.len(), 3);
        assert!(!assembled.canonical_chunks.contains(&edited_coord));
    }
}
