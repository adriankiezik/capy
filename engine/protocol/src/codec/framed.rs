use super::{Error, Limits, Link, Result};
use std::{
    collections::VecDeque,
    io::{ErrorKind, Read, Write},
};

pub struct Framed<S> {
    stream: S,
    limits: Limits,
    header: [u8; 4],
    header_read: usize,
    incoming: Vec<u8>,
    incoming_read: usize,
    outgoing: VecDeque<Vec<u8>>,
    outgoing_offset: usize,
    queued: usize,
}

impl<S> Framed<S> {
    pub fn new(stream: S, limits: Limits) -> Result<Self> {
        limits.validate()?;

        Ok(Self {
            stream,
            limits,
            header: [0; 4],
            header_read: 0,
            incoming: Vec::new(),
            incoming_read: 0,
            outgoing: VecDeque::new(),
            outgoing_offset: 0,
            queued: 0,
        })
    }
}

impl<S: Read + Write + Send> Link for Framed<S> {
    fn send(&mut self, bytes: Vec<u8>) -> Result<()> {
        if bytes.is_empty()
            || bytes.len() > self.limits.max_message_bytes
            || self.outgoing.len() >= self.limits.max_queued_messages
            || bytes.len() + 4 > self.limits.max_queued_bytes.saturating_sub(self.queued)
        {
            return Err(Error::Limit);
        }

        let mut frame = Vec::with_capacity(bytes.len() + 4);

        frame.extend_from_slice(&(bytes.len() as u32).to_be_bytes());

        frame.extend_from_slice(&bytes);

        self.queued += frame.len();

        self.outgoing.push_back(frame);

        self.flush()
    }

    fn receive(&mut self) -> Result<Option<Vec<u8>>> {
        let mut budget = 1024 * 1024;

        loop {
            if self.header_read == 4 && self.incoming.is_empty() {
                let size = u32::from_be_bytes(self.header) as usize;

                if size == 0 || size > self.limits.max_message_bytes {
                    return Err(Error::Limit);
                }

                self.incoming.resize(size, 0);
            }

            if self.header_read == 4 && self.incoming_read == self.incoming.len() {
                self.header_read = 0;
                self.incoming_read = 0;

                return Ok(Some(std::mem::take(&mut self.incoming)));
            }

            if budget == 0 {
                return Ok(None);
            }

            let header = self.header_read < 4;

            let target = if header {
                &mut self.header[self.header_read..]
            } else {
                &mut self.incoming[self.incoming_read..]
            };

            let count = target.len().min(budget);

            match self.stream.read(&mut target[..count]) {
                Ok(0) => return Err(Error::Closed),
                Ok(n) => {
                    if header {
                        self.header_read += n;
                    } else {
                        self.incoming_read += n;
                    }

                    budget -= n;
                }
                Err(error) if error.kind() == ErrorKind::WouldBlock => return Ok(None),
                Err(error) if error.kind() == ErrorKind::Interrupted => continue,
                Err(error) => return Err(error.into()),
            }
        }
    }

    fn flush(&mut self) -> Result<()> {
        let mut budget = 1024 * 1024;

        while let Some(frame) = self.outgoing.front() {
            if budget == 0 {
                break;
            }

            let end = frame.len().min(self.outgoing_offset + budget);

            match self.stream.write(&frame[self.outgoing_offset..end]) {
                Ok(0) => return Err(Error::Closed),
                Ok(n) => {
                    self.outgoing_offset += n;
                    self.queued -= n;
                    budget -= n;
                }
                Err(error) if error.kind() == ErrorKind::WouldBlock => break,
                Err(error) if error.kind() == ErrorKind::Interrupted => continue,
                Err(error) => return Err(error.into()),
            }

            if self.outgoing_offset == frame.len() {
                self.outgoing.pop_front();

                self.outgoing_offset = 0;
            }
        }

        Ok(())
    }
}
