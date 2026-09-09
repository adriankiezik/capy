pub trait Asset: Send + Sync + Sized + 'static {
    fn decode(bytes: Vec<u8>) -> anyhow::Result<Self>;
}

impl Asset for Vec<u8> {
    fn decode(bytes: Vec<u8>) -> anyhow::Result<Self> {
        Ok(bytes)
    }
}
