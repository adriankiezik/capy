
// Branchless — select maps to a hardware cmov, no warp divergence.
fn bit_is_set_64(mask_lo: u32, mask_hi: u32, bit: u32) -> bool {
    let word = select(mask_hi, mask_lo, bit < 32u);
    return (word & (1u << (bit & 31u))) != 0u;
}

// Branchless popcount of all set bits at positions < bit (0..63).
fn popcount_below(mask_lo: u32, mask_hi: u32, bit: u32) -> u32 {
    // lo half: count bits below min(bit, 32). Guard the bit==32 edge
    // where (1u << 32u) wraps to 1 in WGSL's modular shift semantics.
    let lo_bits = min(bit, 32u);
    let lo_mask = select((1u << lo_bits) - 1u, 0xFFFFFFFFu, lo_bits == 32u);
    let lo_count = countOneBits(mask_lo & lo_mask);
    // hi half: count bits in range 32..bit. When bit <= 32 hi_bits is 0
    // and the mask evaluates to 0, contributing nothing.
    let hi_bits = max(bit, 32u) - 32u;
    let hi_mask = (1u << hi_bits) - 1u;
    return lo_count + countOneBits(mask_hi & hi_mask);
}
