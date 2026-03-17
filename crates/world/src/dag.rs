use std::collections::HashMap;
use std::hash::{BuildHasherDefault, Hasher};

use crate::sparse64tree::FlatTree;

const LEAF_DATA_WORDS: u32 = 32;

// Fast integer hasher — uses multiply-by-golden-ratio for u32/u64 keys.
// Much faster than SipHash for integer keys where DoS resistance is unnecessary.
#[derive(Default)]
struct FxHasher(u64);

impl Hasher for FxHasher {
    #[inline]
    fn write(&mut self, bytes: &[u8]) {
        for &b in bytes {
            self.0 = (self.0.rotate_left(5) ^ b as u64).wrapping_mul(0x517cc1b727220a95);
        }
    }

    #[inline]
    fn write_u32(&mut self, i: u32) {
        self.0 = (i as u64).wrapping_mul(0x517cc1b727220a95);
    }

    #[inline]
    fn write_u64(&mut self, i: u64) {
        self.0 = i.wrapping_mul(0x517cc1b727220a95);
    }

    #[inline]
    fn write_usize(&mut self, i: usize) {
        self.0 = (i as u64).wrapping_mul(0x517cc1b727220a95);
    }

    #[inline]
    fn finish(&self) -> u64 {
        self.0
    }
}

type FxBuildHasher = BuildHasherDefault<FxHasher>;
type FxHashMap<K, V> = HashMap<K, V, FxBuildHasher>;

pub(crate) fn reduce_to_dag(flat: FlatTree) -> FlatTree {
    let buf = &flat.buffer;

    if buf.is_empty() {
        return flat;
    }

    let depth = flat.depth as usize;

    let est_nodes = buf.len() / 8;
    let mut ctx = DagReduceCtx {
        old_to_new: FxHashMap::with_capacity_and_hasher(est_nodes, FxBuildHasher::default()),
        level_dedup: (0..=depth)
            .map(|_| {
                FxHashMap::with_capacity_and_hasher(
                    est_nodes / (depth + 1).max(1),
                    FxBuildHasher::default(),
                )
            })
            .collect(),
        node_hash: FxHashMap::with_capacity_and_hasher(est_nodes, FxBuildHasher::default()),
        unique_nodes_ordered: Vec::with_capacity(est_nodes),
        node_depth: FxHashMap::with_capacity_and_hasher(est_nodes, FxBuildHasher::default()),
    };

    dfs_post_order(buf, flat.root_offset, depth, &mut ctx);

    let mut bfs_order = ctx.unique_nodes_ordered.clone();
    bfs_order.sort_by_key(|&off| {
        let d = ctx.node_depth[&off];
        usize::MAX - d
    });

    let mut new_offset_map: FxHashMap<u32, u32> =
        FxHashMap::with_capacity_and_hasher(bfs_order.len(), FxBuildHasher::default());
    let mut cursor: u32 = 0;
    for &old_off in &bfs_order {
        new_offset_map.insert(old_off, cursor);
        let node = read_node(buf, old_off);
        let ws = node_word_size(node.mask_lo, node.mask_hi, node.is_leaf);
        cursor += ws;
    }

    let new_buffer_len = cursor as usize;

    let mut new_buf: Vec<u32> = vec![0u32; new_buffer_len];
    let mut new_avg_color_buf: Vec<u32> = vec![0u32; new_buffer_len];

    for &old_off in &bfs_order {
        let new_off = new_offset_map[&old_off] as usize;
        let node = read_node(buf, old_off);
        let wo = new_off;

        new_buf[wo] = node.mask_lo;
        new_buf[wo + 1] = node.mask_hi;
        new_buf[wo + 2] = node.flags;

        debug_assert!(
            (old_off as usize) < flat.avg_color_buf.len(),
            "avg_color_buf missing entry for node at offset {old_off}"
        );
        let old_avg = flat
            .avg_color_buf
            .get(old_off as usize)
            .copied()
            .unwrap_or(0);
        new_avg_color_buf[wo] = old_avg;

        if node.is_leaf {
            for i in 0..LEAF_DATA_WORDS as usize {
                new_buf[wo + 3 + i] = buf[old_off as usize + 3 + i];
            }
        } else {
            let child_count = (node.mask_lo.count_ones() + node.mask_hi.count_ones()) as usize;
            for i in 0..child_count {
                let old_child_off = buf[old_off as usize + 3 + i];
                let canonical_old = resolve_canonical(&ctx.old_to_new, old_child_off);
                let new_child_off = new_offset_map[&canonical_old];
                new_buf[wo + 3 + i] = new_child_off;
            }
        }
    }

    let canonical_root = resolve_canonical(&ctx.old_to_new, flat.root_offset);
    let new_root_offset = new_offset_map[&canonical_root];

    FlatTree {
        buffer: new_buf,
        avg_color_buf: new_avg_color_buf,
        root_offset: new_root_offset,
        world_size: flat.world_size,
        depth: flat.depth,
    }
}

struct NodeView {
    mask_lo: u32,
    mask_hi: u32,
    flags: u32,
    is_leaf: bool,
}

#[inline]
fn read_node(buf: &[u32], offset: u32) -> NodeView {
    let o = offset as usize;
    debug_assert!(
        o + 3 <= buf.len(),
        "read_node: offset {} out of bounds (buf len {})",
        o,
        buf.len()
    );
    let mask_lo = buf[o];
    let mask_hi = buf[o + 1];
    let flags = buf[o + 2];
    let is_leaf = (flags & 1) != 0;
    let node_size = node_word_size(mask_lo, mask_hi, is_leaf);
    debug_assert!(
        o + node_size as usize <= buf.len(),
        "read_node: node at offset {} with size {} exceeds buffer len {}",
        o,
        node_size,
        buf.len()
    );
    NodeView {
        mask_lo,
        mask_hi,
        flags,
        is_leaf,
    }
}

#[inline]
fn node_word_size(mask_lo: u32, mask_hi: u32, is_leaf: bool) -> u32 {
    if is_leaf {
        3 + LEAF_DATA_WORDS
    } else {
        3 + mask_lo.count_ones() + mask_hi.count_ones()
    }
}

const FNV_OFFSET: u64 = 0xcbf29ce484222325;
const FNV_PRIME: u64 = 0x00000100000001b3;

#[inline]
fn fnv_mix(mut hash: u64, word: u32) -> u64 {
    let bytes = word.to_le_bytes();
    for b in bytes {
        hash ^= b as u64;
        hash = hash.wrapping_mul(FNV_PRIME);
    }
    hash
}

#[inline]
fn hash_leaf(buf: &[u32], offset: u32) -> u64 {
    let o = offset as usize;
    let mut h = FNV_OFFSET;
    h = fnv_mix(h, buf[o]);
    h = fnv_mix(h, buf[o + 1]);
    for i in 0..LEAF_DATA_WORDS as usize {
        h = fnv_mix(h, buf[o + 3 + i]);
    }
    h
}

#[inline]
fn hash_inner(buf: &[u32], offset: u32, node_hash: &FxHashMap<u32, u64>) -> u64 {
    let o = offset as usize;
    let mask_lo = buf[o];
    let mask_hi = buf[o + 1];
    let mut h = FNV_OFFSET;
    h = fnv_mix(h, mask_lo);
    h = fnv_mix(h, mask_hi);

    let mut combined = (mask_lo as u64) | ((mask_hi as u64) << 32);
    let mut child_packed_idx: usize = 0;
    while combined != 0 {
        let bit = combined.trailing_zeros();
        let child_off = buf[o + 3 + child_packed_idx];
        debug_assert!(
            node_hash.contains_key(&child_off),
            "child at offset {child_off} was not hashed before parent"
        );
        let child_hash = node_hash[&child_off];
        h = fnv_mix(h, (child_hash & 0xFFFF_FFFF) as u32);
        h = fnv_mix(h, ((child_hash >> 32) & 0xFFFF_FFFF) as u32);
        h = fnv_mix(h, bit);
        child_packed_idx += 1;
        combined &= combined - 1;
    }
    h
}

#[inline]
fn resolve_canonical(old_to_new: &FxHashMap<u32, u32>, off: u32) -> u32 {
    debug_assert!(
        old_to_new.contains_key(&off),
        "node at offset {off} was never visited during DFS"
    );
    old_to_new[&off]
}

struct DagReduceCtx {
    old_to_new: FxHashMap<u32, u32>,
    level_dedup: Vec<FxHashMap<u64, u32>>,
    node_hash: FxHashMap<u32, u64>,
    unique_nodes_ordered: Vec<u32>,
    node_depth: FxHashMap<u32, usize>,
}

fn dfs_post_order(buf: &[u32], offset: u32, remaining_depth: usize, ctx: &mut DagReduceCtx) {
    if ctx.old_to_new.contains_key(&offset) {
        return;
    }

    let o = offset as usize;
    let mask_lo = buf[o];
    let mask_hi = buf[o + 1];
    let flags = buf[o + 2];
    let is_leaf = (flags & 1) != 0;

    if !is_leaf {
        let child_count = (mask_lo.count_ones() + mask_hi.count_ones()) as usize;
        for i in 0..child_count {
            let child_off = buf[o + 3 + i];
            dfs_post_order(buf, child_off, remaining_depth.saturating_sub(1), ctx);
        }
    }

    let h = if is_leaf {
        hash_leaf(buf, offset)
    } else {
        hash_inner(buf, offset, &ctx.node_hash)
    };
    ctx.node_hash.insert(offset, h);

    ctx.node_depth.insert(offset, remaining_depth);

    let level_idx = remaining_depth.min(ctx.level_dedup.len() - 1);
    match ctx.level_dedup[level_idx].get(&h).copied() {
        Some(canonical_old_offset) => {
            ctx.old_to_new.insert(offset, canonical_old_offset);
        }
        None => {
            ctx.level_dedup[level_idx].insert(h, offset);
            ctx.old_to_new.insert(offset, offset);
            ctx.unique_nodes_ordered.push(offset);
        }
    }
}
