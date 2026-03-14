pub(crate) fn content_hash(data: &[u8]) -> [u8; 16] {
    xxhash_rust::xxh3::xxh3_128(data).to_le_bytes()
}
