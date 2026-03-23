use capy_core::{BakedChunkData, MaterialId, is_foliage_material};

use crate::sparse64tree::{compute_leaf_avg_color, local_to_bit};

const BRANCH: u32 = 4;
const BRANCH_CUBED: usize = (BRANCH * BRANCH * BRANCH) as usize;
const LEAF_DATA_WORDS: usize = BRANCH_CUBED / 2;
const HEADER_WORDS: usize = 3; // mask_lo, mask_hi, flags

/// A single voxel change within a chunk's local coordinate space.
pub struct VoxelEdit {
    pub x: u32,
    pub y: u32,
    pub z: u32,
    pub material: MaterialId,
}

/// A complete leaf-brick edit: all 64 materials for a 4×4×4 block.
pub struct LeafBrickEdit {
    pub bx: u32,
    pub by: u32,
    pub bz: u32,
    pub materials: [MaterialId; BRANCH_CUBED],
}

/// Patch an existing BakedChunkData by modifying only the tree nodes
/// affected by the given edits. Append-only: old nodes stay in the buffer,
/// new/modified nodes are appended at the end.
pub fn patch_baked_chunk(old: &BakedChunkData, edits: &[VoxelEdit]) -> BakedChunkData {
    if edits.is_empty() {
        return old.clone();
    }

    let mut buf = old.dag_buffer.clone();
    let mut avg_buf = old.avg_color_buffer.clone();
    let mut root = old.root_offset;
    let world_size = old.world_size;
    let depth = old.depth;

    // Group edits by leaf block coordinate (x/4, y/4, z/4).
    let mut leaf_groups: std::collections::HashMap<(u32, u32, u32), Vec<&VoxelEdit>> =
        std::collections::HashMap::new();
    for edit in edits {
        let key = (edit.x / BRANCH, edit.y / BRANCH, edit.z / BRANCH);
        leaf_groups.entry(key).or_default().push(edit);
    }

    // Process each leaf block group. Each group produces a new root.
    for ((bx, by, bz), group) in &leaf_groups {
        root = patch_one_leaf_block(
            &mut buf,
            &mut avg_buf,
            root,
            world_size,
            depth,
            *bx,
            *by,
            *bz,
            group,
        );
    }

    // Ensure avg_buf matches buf length
    avg_buf.resize(buf.len(), 0);

    // Conservatively expand foliage Y range based on edits (never shrink).
    let mut foliage_y_min = old.foliage_y_min;
    let mut foliage_y_max = old.foliage_y_max;
    for edit in edits {
        if is_foliage_material(edit.material) {
            if foliage_y_min >= foliage_y_max {
                // No foliage before — initialize range.
                foliage_y_min = edit.y;
                foliage_y_max = edit.y + 1;
            } else {
                foliage_y_min = foliage_y_min.min(edit.y);
                foliage_y_max = foliage_y_max.max(edit.y + 1);
            }
        }
    }

    BakedChunkData {
        dag_buffer: buf,
        avg_color_buffer: avg_buf,
        root_offset: root,
        world_size,
        depth,
        foliage_y_min,
        foliage_y_max,
        // Bitmap and bands are recomputed by compact_baked_chunk after patching.
        foliage_bitmap: None,
        foliage_y_bands: 0,
        foliage_tile_y_ranges: None,
    }
}

/// Patch an existing BakedChunkData using pre-grouped leaf brick edits.
/// Uses batched recursive top-down traversal: each inner node is created
/// at most once, regardless of how many leaves are modified.
///
/// The `bricks` slice is sorted in-place by tree-path key for the recursive
/// partitioning to work correctly.
pub fn patch_baked_chunk_bricks(
    old: &BakedChunkData,
    bricks: &mut [LeafBrickEdit],
) -> BakedChunkData {
    patch_baked_chunk_bricks_owned(old.clone(), bricks)
}

/// Patch an existing baked chunk, taking ownership so the caller can reuse the
/// existing backing buffers instead of cloning them up front.
pub fn patch_baked_chunk_bricks_owned(
    old: BakedChunkData,
    bricks: &mut [LeafBrickEdit],
) -> BakedChunkData {
    if bricks.is_empty() {
        return old;
    }

    let world_size = old.world_size;
    let depth = old.depth;
    let root_offset = old.root_offset;
    let mut buf = old.dag_buffer;
    let mut avg_buf = old.avg_color_buffer;

    // Sort bricks by tree-path key so children with the same parent are contiguous.
    let blocks_per_leaf = leaf_blocks_per_branch(world_size, depth);
    bricks.sort_unstable_by_key(|b| brick_sort_key(b.bx, b.by, b.bz, blocks_per_leaf, depth));

    let new_root = patch_subtree(
        &mut buf,
        &mut avg_buf,
        root_offset,
        world_size,
        depth,
        blocks_per_leaf,
        bricks,
    );

    avg_buf.resize(buf.len(), 0);

    // Conservatively expand foliage Y range based on brick edits (never shrink).
    let mut foliage_y_min = old.foliage_y_min;
    let mut foliage_y_max = old.foliage_y_max;
    for brick in bricks.iter() {
        let has_foliage = brick.materials.iter().any(|&m| is_foliage_material(m));
        if has_foliage {
            let brick_y_min = brick.by * BRANCH;
            let brick_y_max = brick_y_min + BRANCH;
            if foliage_y_min >= foliage_y_max {
                foliage_y_min = brick_y_min;
                foliage_y_max = brick_y_max;
            } else {
                foliage_y_min = foliage_y_min.min(brick_y_min);
                foliage_y_max = foliage_y_max.max(brick_y_max);
            }
        }
    }

    BakedChunkData {
        dag_buffer: buf,
        avg_color_buffer: avg_buf,
        root_offset: new_root,
        world_size,
        depth,
        foliage_y_min,
        foliage_y_max,
        // Bitmap and bands are recomputed by compact_baked_chunk after patching.
        foliage_bitmap: None,
        foliage_y_bands: 0,
        foliage_tile_y_ranges: None,
    }
}

/// How many leaf blocks fit in one BRANCH-unit at the root level.
/// For depth D and world_size W: blocks_per_branch = W / (BRANCH^1) / BRANCH^(D-2)
/// Simplified: (W / BRANCH) / BRANCH^(D-2) = W / BRANCH^(D-1)
/// But we want "leaf blocks per child at root": world_size / BRANCH in leaf-block coords
/// is world_size / BRANCH / 1 = world_size / BRANCH.
/// Actually, leaf blocks per root child = (world_size / BRANCH)^3 blocks total under one child.
/// We need blocks_per_child_1d at each level.
fn leaf_blocks_per_branch(_world_size: u32, depth: u32) -> u32 {
    // At each level above leaves, blocks_per_child = (parent_blocks_per_child / BRANCH).
    // At root level, world_size (in leaf blocks) = world_size / BRANCH (since leaves are already BRANCH-sized).
    // Wait — world_size is in voxels/BRANCH = leaf blocks.
    // No, world_size is the padded voxel dimension / BRANCH (since each leaf = 4 voxels).
    // The tree has `depth` levels. Level 1 = leaf. Level depth = root.
    // At level L, each node covers (BRANCH^(L-1)) leaf blocks per axis.
    // At the root (level=depth), each child covers BRANCH^(depth-2) leaf blocks per axis.
    // So blocks_per_child at root = BRANCH^(depth-2).
    if depth <= 1 { 1 } else { BRANCH.pow(depth - 2) }
}

/// Compute a sort key for a brick so that bricks sharing tree-path prefixes are contiguous.
/// The key encodes the child index at each level from root to leaf-parent.
fn brick_sort_key(bx: u32, by: u32, bz: u32, blocks_per_branch: u32, depth: u32) -> u64 {
    if depth <= 1 {
        return 0;
    }

    let mut key: u64 = 0;
    let mut bpc = blocks_per_branch; // blocks per child at current level

    // depth-1 inner levels (root down to leaf parent)
    for _ in 0..(depth - 1) {
        let lx = if bpc > 0 {
            (bx / bpc) % BRANCH
        } else {
            bx % BRANCH
        };
        let ly = if bpc > 0 {
            (by / bpc) % BRANCH
        } else {
            by % BRANCH
        };
        let lz = if bpc > 0 {
            (bz / bpc) % BRANCH
        } else {
            bz % BRANCH
        };
        key = (key << 6) | (local_to_bit(lx, ly, lz) as u64);
        bpc /= BRANCH;
    }
    key
}

/// Recursively patch a subtree, returning the offset of the (possibly new) node.
///
/// `bricks` must be sorted by `brick_sort_key` and contain only bricks that
/// fall within this subtree.
fn patch_subtree(
    buf: &mut Vec<u32>,
    avg_buf: &mut Vec<u32>,
    old_offset: u32,
    _node_size: u32,
    remaining_depth: u32,
    blocks_per_child: u32,
    bricks: &[LeafBrickEdit],
) -> u32 {
    if bricks.is_empty() {
        return old_offset;
    }

    // Leaf level: remaining_depth == 1
    if remaining_depth == 1 {
        // Use the last brick's materials (there should be exactly one brick per leaf).
        let brick = &bricks[bricks.len() - 1];
        let mut mask = 0u64;
        for (i, &mat) in brick.materials.iter().enumerate() {
            if mat != 0 {
                mask |= 1u64 << i;
            }
        }
        return append_leaf(buf, avg_buf, mask, &brick.materials);
    }

    // Inner node: partition bricks by which child they belong to.
    let o = old_offset as usize;
    let old_mask = (buf[o] as u64) | ((buf[o + 1] as u64) << 32);

    // Read old child pointers into a fixed-size array.
    let old_child_count = old_mask.count_ones() as usize;
    let mut old_ptrs = [0u32; BRANCH_CUBED];
    for i in 0..old_child_count {
        old_ptrs[i] = buf[o + HEADER_WORDS + i];
    }

    // Partition bricks by child index. Since bricks are sorted by tree-path key,
    // bricks for the same child are contiguous.
    // We iterate once and find the ranges.
    let child_bpc = if blocks_per_child > 0 {
        blocks_per_child
    } else {
        1
    };

    // Build array of (child_bit_index, start, end) for each contiguous group.
    let mut groups = Vec::with_capacity(BRANCH_CUBED);
    {
        let mut i = 0;
        while i < bricks.len() {
            let b = &bricks[i];
            let lx = if child_bpc > 0 {
                (b.bx / child_bpc) % BRANCH
            } else {
                b.bx % BRANCH
            };
            let ly = if child_bpc > 0 {
                (b.by / child_bpc) % BRANCH
            } else {
                b.by % BRANCH
            };
            let lz = if child_bpc > 0 {
                (b.bz / child_bpc) % BRANCH
            } else {
                b.bz % BRANCH
            };
            let bit = local_to_bit(lx, ly, lz);
            let start = i;
            i += 1;
            while i < bricks.len() {
                let b2 = &bricks[i];
                let lx2 = if child_bpc > 0 {
                    (b2.bx / child_bpc) % BRANCH
                } else {
                    b2.bx % BRANCH
                };
                let ly2 = if child_bpc > 0 {
                    (b2.by / child_bpc) % BRANCH
                } else {
                    b2.by % BRANCH
                };
                let lz2 = if child_bpc > 0 {
                    (b2.bz / child_bpc) % BRANCH
                } else {
                    b2.bz % BRANCH
                };
                let bit2 = local_to_bit(lx2, ly2, lz2);
                if bit2 != bit {
                    break;
                }
                i += 1;
            }
            groups.push((bit, start, i));
        }
    }

    // Build new mask: old mask | all modified child bits.
    let mut new_mask = old_mask;
    for &(bit, _, _) in &groups {
        new_mask |= 1u64 << bit;
    }
    let new_child_count = new_mask.count_ones() as usize;

    // Build new child pointer array. Start with old pointers mapped to new positions.
    let mut new_ptrs = [0u32; BRANCH_CUBED];
    {
        // Map old pointers: iterate set bits of old_mask, assign to new packed positions.
        let mut old_idx = 0usize;
        let mut remaining = old_mask;
        while remaining != 0 {
            let bit = remaining.trailing_zeros();
            remaining &= remaining - 1;
            // New packed index for this bit in new_mask
            let below = if bit == 0 {
                0u64
            } else {
                new_mask & ((1u64 << bit) - 1)
            };
            let new_idx = below.count_ones() as usize;
            new_ptrs[new_idx] = old_ptrs[old_idx];
            old_idx += 1;
        }
    }

    // Recurse into modified children and update their pointers.
    let next_bpc = blocks_per_child / BRANCH;
    let child_node_size = _node_size / BRANCH;

    for &(bit, start, end) in &groups {
        let below = if bit == 0 {
            0u64
        } else {
            new_mask & ((1u64 << bit) - 1)
        };
        let packed_idx = below.count_ones() as usize;

        let child_existed = (old_mask & (1u64 << bit)) != 0;
        let old_child_off = if child_existed {
            new_ptrs[packed_idx]
        } else {
            // Child didn't exist — create an empty inner node for recursion.
            // Actually, for a non-existent subtree we need to handle differently:
            // Just create leaves directly.
            0
        };

        let new_child_off = if child_existed {
            patch_subtree(
                buf,
                avg_buf,
                old_child_off,
                child_node_size,
                remaining_depth - 1,
                next_bpc,
                &bricks[start..end],
            )
        } else {
            // No existing subtree — build fresh from scratch.
            build_fresh_subtree(
                buf,
                avg_buf,
                child_node_size,
                remaining_depth - 1,
                next_bpc,
                &bricks[start..end],
            )
        };

        new_ptrs[packed_idx] = new_child_off;
    }

    // Append the new inner node.
    let new_offset = buf.len() as u32;
    buf.push(new_mask as u32);
    buf.push((new_mask >> 32) as u32);
    buf.push(0); // flags: inner

    for &ptr in &new_ptrs[..new_child_count] {
        buf.push(ptr);
    }

    // Recompute avg color from children.
    recompute_avg_color(buf, avg_buf, new_offset, new_child_count);

    new_offset
}

/// Build a fresh subtree for bricks that have no existing parent node.
fn build_fresh_subtree(
    buf: &mut Vec<u32>,
    avg_buf: &mut Vec<u32>,
    _node_size: u32,
    remaining_depth: u32,
    blocks_per_child: u32,
    bricks: &[LeafBrickEdit],
) -> u32 {
    if bricks.is_empty() {
        // Shouldn't happen, but return a dummy empty node.
        let offset = buf.len() as u32;
        buf.push(0);
        buf.push(0);
        buf.push(0);
        avg_buf.resize(buf.len(), 0);
        return offset;
    }

    if remaining_depth == 1 {
        let brick = &bricks[bricks.len() - 1];
        let mut mask = 0u64;
        for (i, &mat) in brick.materials.iter().enumerate() {
            if mat != 0 {
                mask |= 1u64 << i;
            }
        }
        return append_leaf(buf, avg_buf, mask, &brick.materials);
    }

    // Partition bricks by child slot.
    let child_bpc = if blocks_per_child > 0 {
        blocks_per_child
    } else {
        1
    };

    let mut groups = Vec::with_capacity(BRANCH_CUBED);
    {
        let mut i = 0;
        while i < bricks.len() {
            let b = &bricks[i];
            let lx = if child_bpc > 0 {
                (b.bx / child_bpc) % BRANCH
            } else {
                b.bx % BRANCH
            };
            let ly = if child_bpc > 0 {
                (b.by / child_bpc) % BRANCH
            } else {
                b.by % BRANCH
            };
            let lz = if child_bpc > 0 {
                (b.bz / child_bpc) % BRANCH
            } else {
                b.bz % BRANCH
            };
            let bit = local_to_bit(lx, ly, lz);
            let start = i;
            i += 1;
            while i < bricks.len() {
                let b2 = &bricks[i];
                let lx2 = if child_bpc > 0 {
                    (b2.bx / child_bpc) % BRANCH
                } else {
                    b2.bx % BRANCH
                };
                let ly2 = if child_bpc > 0 {
                    (b2.by / child_bpc) % BRANCH
                } else {
                    b2.by % BRANCH
                };
                let lz2 = if child_bpc > 0 {
                    (b2.bz / child_bpc) % BRANCH
                } else {
                    b2.bz % BRANCH
                };
                let bit2 = local_to_bit(lx2, ly2, lz2);
                if bit2 != bit {
                    break;
                }
                i += 1;
            }
            groups.push((bit, start, i));
        }
    }

    let mut mask = 0u64;
    for &(bit, _, _) in &groups {
        mask |= 1u64 << bit;
    }
    let child_count = mask.count_ones() as usize;

    let next_bpc = blocks_per_child / BRANCH;
    let child_node_size = _node_size / BRANCH;

    // Build children first, collect offsets.
    let mut child_offsets = [0u32; BRANCH_CUBED];
    for &(bit, start, end) in &groups {
        let below = if bit == 0 {
            0u64
        } else {
            mask & ((1u64 << bit) - 1)
        };
        let packed_idx = below.count_ones() as usize;

        child_offsets[packed_idx] = build_fresh_subtree(
            buf,
            avg_buf,
            child_node_size,
            remaining_depth - 1,
            next_bpc,
            &bricks[start..end],
        );
    }

    // Append inner node.
    let offset = buf.len() as u32;
    buf.push(mask as u32);
    buf.push((mask >> 32) as u32);
    buf.push(0); // flags: inner

    for &ptr in &child_offsets[..child_count] {
        buf.push(ptr);
    }

    recompute_avg_color(buf, avg_buf, offset, child_count);

    offset
}

// ---- Existing per-voxel patcher (kept for backward compatibility) ----

/// Entry in the navigation path from root to leaf.
struct PathEntry {
    offset: Option<u32>,
    bit_index: u32,
    packed_idx: u32,
    child_exists: bool,
}

/// Patch one leaf block and return the new root offset.
#[allow(clippy::too_many_arguments)]
fn patch_one_leaf_block(
    buf: &mut Vec<u32>,
    avg_buf: &mut Vec<u32>,
    root: u32,
    world_size: u32,
    depth: u32,
    bx: u32,
    by: u32,
    bz: u32,
    edits: &[&VoxelEdit],
) -> u32 {
    let path = navigate_path(buf, root, depth, world_size, bx, by, bz);

    let leaf_entry = path.last();
    let old_materials = if let Some(entry) = leaf_entry {
        if entry.child_exists {
            if let Some(inner_off) = entry.offset {
                let child_off = buf[inner_off as usize + HEADER_WORDS + entry.packed_idx as usize];
                read_leaf_materials(buf, child_off)
            } else {
                [0 as MaterialId; BRANCH_CUBED]
            }
        } else {
            [0 as MaterialId; BRANCH_CUBED]
        }
    } else {
        read_leaf_materials(buf, root)
    };

    let mut materials = old_materials;
    for edit in edits {
        let lx = edit.x % BRANCH;
        let ly = edit.y % BRANCH;
        let lz = edit.z % BRANCH;
        let bit = local_to_bit(lx, ly, lz);
        materials[bit as usize] = edit.material;
    }

    let mut new_mask = 0u64;
    for (i, &mat) in materials.iter().enumerate() {
        if mat != 0 {
            new_mask |= 1u64 << i;
        }
    }

    if path.is_empty() {
        let new_leaf = append_leaf(buf, avg_buf, new_mask, &materials);
        return new_leaf;
    }

    let new_child_off = append_leaf(buf, avg_buf, new_mask, &materials);

    let mut current_child_off = new_child_off;
    for entry in path.iter().rev() {
        current_child_off = append_inner_with_update(buf, avg_buf, entry, current_child_off);
    }

    current_child_off
}

fn navigate_path(
    buf: &[u32],
    root: u32,
    depth: u32,
    world_size: u32,
    bx: u32,
    by: u32,
    bz: u32,
) -> Vec<PathEntry> {
    if depth <= 1 {
        return Vec::new();
    }

    let mut path = Vec::with_capacity(depth as usize - 1);
    let mut current_off: Option<u32> = Some(root);
    let mut current_size = world_size;

    for _ in 0..(depth - 1) {
        let blocks_per_child = current_size / BRANCH / BRANCH;

        let lx = if blocks_per_child > 0 {
            (bx / blocks_per_child) % BRANCH
        } else {
            bx % BRANCH
        };
        let ly = if blocks_per_child > 0 {
            (by / blocks_per_child) % BRANCH
        } else {
            by % BRANCH
        };
        let lz = if blocks_per_child > 0 {
            (bz / blocks_per_child) % BRANCH
        } else {
            bz % BRANCH
        };
        let bit_index = local_to_bit(lx, ly, lz);

        if let Some(off) = current_off {
            let o = off as usize;
            let mask = (buf[o] as u64) | ((buf[o + 1] as u64) << 32);
            let child_exists = (mask & (1u64 << bit_index)) != 0;

            let below_mask = if bit_index == 0 {
                0u64
            } else {
                mask & ((1u64 << bit_index) - 1)
            };
            let packed_idx = below_mask.count_ones();

            path.push(PathEntry {
                offset: Some(off),
                bit_index,
                packed_idx,
                child_exists,
            });

            if child_exists {
                current_off = Some(buf[o + HEADER_WORDS + packed_idx as usize]);
            } else {
                current_off = None;
            }
        } else {
            path.push(PathEntry {
                offset: None,
                bit_index,
                packed_idx: 0,
                child_exists: false,
            });
        }

        current_size /= BRANCH;
    }

    path
}

fn read_leaf_materials(buf: &[u32], leaf_offset: u32) -> [MaterialId; BRANCH_CUBED] {
    let o = leaf_offset as usize;
    let mut materials = [0 as MaterialId; BRANCH_CUBED];

    for word_idx in 0..LEAF_DATA_WORDS {
        let word = buf[o + HEADER_WORDS + word_idx];
        let base = word_idx * 2;
        materials[base] = (word & 0xFFFF) as MaterialId;
        materials[base + 1] = (word >> 16) as MaterialId;
    }

    materials
}

fn append_leaf(
    buf: &mut Vec<u32>,
    avg_buf: &mut Vec<u32>,
    mask: u64,
    materials: &[MaterialId; BRANCH_CUBED],
) -> u32 {
    let offset = buf.len() as u32;

    buf.push(mask as u32);
    buf.push((mask >> 32) as u32);
    buf.push(1); // flags: is_leaf = 1

    for chunk in materials.chunks(2) {
        let first = chunk[0] as u32;
        let second = chunk[1] as u32;
        buf.push(first | (second << 16));
    }

    let avg_color = compute_leaf_avg_color(materials, mask);
    avg_buf.resize(buf.len(), 0);
    let avg_word =
        (avg_color[0] as u32) | ((avg_color[1] as u32) << 8) | ((avg_color[2] as u32) << 16);
    avg_buf[offset as usize] = avg_word;

    offset
}

fn append_inner_with_update(
    buf: &mut Vec<u32>,
    avg_buf: &mut Vec<u32>,
    entry: &PathEntry,
    new_child_off: u32,
) -> u32 {
    if let Some(off) = entry.offset {
        let o = off as usize;
        let old_mask = (buf[o] as u64) | ((buf[o + 1] as u64) << 32);
        let old_child_count = old_mask.count_ones() as usize;

        let new_mask = old_mask | (1u64 << entry.bit_index);
        let new_child_count = new_mask.count_ones() as usize;

        let new_offset = buf.len() as u32;
        buf.push(new_mask as u32);
        buf.push((new_mask >> 32) as u32);
        buf.push(0);

        let old_ptrs: Vec<u32> = (0..old_child_count)
            .map(|i| buf[o + HEADER_WORDS + i])
            .collect();

        let packed_idx = entry.packed_idx as usize;

        if entry.child_exists {
            for (i, &ptr) in old_ptrs.iter().enumerate() {
                if i == packed_idx {
                    buf.push(new_child_off);
                } else {
                    buf.push(ptr);
                }
            }
        } else {
            for (i, &ptr) in old_ptrs.iter().enumerate() {
                if i == packed_idx {
                    buf.push(new_child_off);
                }
                buf.push(ptr);
            }
            if packed_idx >= old_ptrs.len() {
                buf.push(new_child_off);
            }
            debug_assert_eq!(
                buf.len() - new_offset as usize - HEADER_WORDS,
                new_child_count
            );
        }

        recompute_avg_color(buf, avg_buf, new_offset, new_child_count);

        new_offset
    } else {
        let new_mask = 1u64 << entry.bit_index;

        let new_offset = buf.len() as u32;
        buf.push(new_mask as u32);
        buf.push((new_mask >> 32) as u32);
        buf.push(0);
        buf.push(new_child_off);

        avg_buf.resize(buf.len(), 0);
        let child_avg = avg_buf.get(new_child_off as usize).copied().unwrap_or(0);
        avg_buf[new_offset as usize] = child_avg;

        new_offset
    }
}

fn recompute_avg_color(buf: &[u32], avg_buf: &mut Vec<u32>, node_offset: u32, child_count: usize) {
    let mut color_sum = [0.0f32; 3];
    avg_buf.resize(buf.len(), 0);
    for i in 0..child_count {
        let child_off = buf[node_offset as usize + HEADER_WORDS + i];
        let child_avg = avg_buf.get(child_off as usize).copied().unwrap_or(0);
        color_sum[0] += (child_avg & 0xFF) as f32;
        color_sum[1] += ((child_avg >> 8) & 0xFF) as f32;
        color_sum[2] += ((child_avg >> 16) & 0xFF) as f32;
    }
    if child_count > 0 {
        let n = child_count as f32;
        let avg_word = ((color_sum[0] / n).round() as u32)
            | (((color_sum[1] / n).round() as u32) << 8)
            | (((color_sum[2] / n).round() as u32) << 16);
        avg_buf[node_offset as usize] = avg_word;
    }
}

// ---------------------------------------------------------------------------
// Leaf brick extraction — traverse DAG to recover editable brick data
// ---------------------------------------------------------------------------

/// Extract all leaf bricks from a baked chunk DAG.
///
/// Returns `(bx, by, bz, materials)` tuples for every occupied leaf node.
/// Only bricks that differ from all-zero are returned.
pub fn extract_leaf_bricks(baked: &BakedChunkData) -> Vec<LeafBrickEdit> {
    let buf = &baked.dag_buffer;
    if buf.is_empty() {
        return Vec::new();
    }

    let mut out = Vec::new();

    // Stack entries: (node_offset, remaining_depth, base_bx, base_by, base_bz, blocks_at_level)
    // blocks_at_level = how many leaf blocks one child at this level spans per axis.
    let top_blocks = baked.world_size / BRANCH as u32;
    let mut stack: Vec<(u32, u32, u32, u32, u32, u32)> =
        vec![(baked.root_offset, baked.depth, 0, 0, 0, top_blocks)];

    while let Some((offset, depth, base_bx, base_by, base_bz, blocks)) = stack.pop() {
        let o = offset as usize;
        if o + HEADER_WORDS > buf.len() {
            continue;
        }

        let mask = (buf[o] as u64) | ((buf[o + 1] as u64) << 32);
        let is_leaf = (buf[o + 2] & 1) != 0;

        if is_leaf || depth <= 1 {
            let materials = read_leaf_materials(buf, offset);
            if materials.iter().any(|&m| m != 0) {
                out.push(LeafBrickEdit {
                    bx: base_bx,
                    by: base_by,
                    bz: base_bz,
                    materials,
                });
            }
            continue;
        }

        // Inner node — iterate set bits and push children.
        let child_blocks = blocks / BRANCH as u32;
        let mut remaining = mask;
        let mut packed_idx = 0usize;
        while remaining != 0 {
            let bit = remaining.trailing_zeros();
            remaining &= remaining - 1;

            let lz = bit / (BRANCH * BRANCH);
            let ly = (bit / BRANCH) % BRANCH;
            let lx = bit % BRANCH;

            let child_off = buf[o + HEADER_WORDS + packed_idx];
            packed_idx += 1;

            stack.push((
                child_off,
                depth - 1,
                base_bx + lx * child_blocks,
                base_by + ly * child_blocks,
                base_bz + lz * child_blocks,
                child_blocks,
            ));
        }
    }

    out
}
