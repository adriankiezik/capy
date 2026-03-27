
fn bit_is_set_64(mask_lo: u32, mask_hi: u32, bit: u32) -> bool {
    if bit < 32u {
        return (mask_lo & (1u << bit)) != 0u;
    } else {
        return (mask_hi & (1u << (bit - 32u))) != 0u;
    }
}

fn popcount_below(mask_lo: u32, mask_hi: u32, bit: u32) -> u32 {
    if bit == 0u { return 0u; }
    if bit < 32u {
        return countOneBits(mask_lo & ((1u << bit) - 1u));
    }
    let lo_count = countOneBits(mask_lo);
    if bit == 32u { return lo_count; }
    return lo_count + countOneBits(mask_hi & ((1u << (bit - 32u)) - 1u));
}
