use std::io::{self, Read};

pub(super) fn read_u8(r: &mut impl Read) -> io::Result<u8> {
    let mut buf = [0u8; 1];
    r.read_exact(&mut buf)?;
    Ok(buf[0])
}

pub(super) fn read_u16_le(r: &mut impl Read) -> io::Result<u16> {
    let mut buf = [0u8; 2];
    r.read_exact(&mut buf)?;
    Ok(u16::from_le_bytes(buf))
}

pub(super) fn read_u32_le(r: &mut impl Read) -> io::Result<u32> {
    let mut buf = [0u8; 4];
    r.read_exact(&mut buf)?;
    Ok(u32::from_le_bytes(buf))
}

pub(super) fn read_bytes<const N: usize>(r: &mut impl Read) -> io::Result<[u8; N]> {
    let mut buf = [0u8; N];
    r.read_exact(&mut buf)?;
    Ok(buf)
}
