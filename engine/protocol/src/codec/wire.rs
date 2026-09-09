use super::{Error, Limits, Result};
use serde::{Serialize, de::DeserializeOwned};

pub trait Link: Send {
    fn send(&mut self, message: Vec<u8>) -> Result<()>;

    fn receive(&mut self) -> Result<Option<Vec<u8>>>;

    fn flush(&mut self) -> Result<()>;
}

pub fn encode<T: Serialize>(message: &T, limits: Limits) -> Result<Vec<u8>> {
    struct Bounded {
        bytes: Vec<u8>,
        maximum: usize,
    }

    impl std::io::Write for Bounded {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            if bytes.len() > self.maximum - self.bytes.len() {
                return Err(std::io::Error::other("message budget exceeded"));
            }

            self.bytes.extend_from_slice(bytes);

            Ok(bytes.len())
        }

        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    let mut writer = Bounded {
        bytes: Vec::new(),
        maximum: limits.max_message_bytes,
    };

    serde_json::to_writer(&mut writer, message)?;

    Ok(writer.bytes)
}

pub fn decode<T: DeserializeOwned>(bytes: &[u8], limits: Limits) -> Result<T> {
    if bytes.len() > limits.max_message_bytes {
        return Err(Error::Limit);
    }

    Ok(serde_json::from_slice(bytes)?)
}
