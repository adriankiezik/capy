use crate::error::{AssetError, Result};

pub(crate) fn compress_lz4(data: &[u8]) -> Vec<u8> {
    lz4_flex::compress_prepend_size(data)
}

pub(crate) fn decompress_lz4(data: &[u8]) -> Result<Vec<u8>> {
    lz4_flex::decompress_size_prepended(data)
        .map_err(|e| AssetError::DecompressFailed(e.to_string()))
}
