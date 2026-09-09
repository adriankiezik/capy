#[derive(Clone, Copy, Debug)]
pub struct Limits {
    pub max_message_bytes: usize,
    pub max_queued_messages: usize,
    pub max_queued_bytes: usize,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            max_message_bytes: 32 * 1024 * 1024,
            max_queued_messages: 32,
            max_queued_bytes: 64 * 1024 * 1024,
        }
    }
}

impl Limits {
    pub fn validate(self) -> super::Result<()> {
        if self.max_message_bytes == 0
            || self.max_message_bytes > u32::MAX as usize
            || self.max_queued_messages == 0
            || self.max_queued_bytes < self.max_message_bytes
        {
            return Err(super::Error::Limit);
        }

        Ok(())
    }
}
