use super::{Error, Limits, Link, Result};
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
    mpsc::{self, Receiver, SyncSender},
};

struct Local {
    sender: SyncSender<Vec<u8>>,
    receiver: Receiver<Vec<u8>>,
    outgoing: Arc<AtomicUsize>,
    incoming: Arc<AtomicUsize>,
    limits: Limits,
}

pub fn local_pair(limits: Limits) -> Result<(Box<dyn Link>, Box<dyn Link>)> {
    limits.validate()?;

    let (a, ar) = mpsc::sync_channel(limits.max_queued_messages);

    let (b, br) = mpsc::sync_channel(limits.max_queued_messages);

    let ab = Arc::new(AtomicUsize::new(0));

    let ba = Arc::new(AtomicUsize::new(0));

    Ok((
        Box::new(Local {
            sender: a,
            receiver: br,
            outgoing: ab.clone(),
            incoming: ba.clone(),
            limits,
        }),
        Box::new(Local {
            sender: b,
            receiver: ar,
            outgoing: ba,
            incoming: ab,
            limits,
        }),
    ))
}

impl Link for Local {
    fn send(&mut self, message: Vec<u8>) -> Result<()> {
        let size = message.len();

        if size > self.limits.max_message_bytes {
            return Err(Error::Limit);
        }

        self.outgoing
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |bytes| {
                bytes
                    .checked_add(size)
                    .filter(|&bytes| bytes <= self.limits.max_queued_bytes)
            })
            .map_err(|_| Error::Limit)?;

        if let Err(error) = self.sender.try_send(message) {
            self.outgoing.fetch_sub(size, Ordering::AcqRel);

            return Err(match error {
                mpsc::TrySendError::Full(_) => Error::Limit,
                mpsc::TrySendError::Disconnected(_) => Error::Closed,
            });
        }

        Ok(())
    }

    fn receive(&mut self) -> Result<Option<Vec<u8>>> {
        match self.receiver.try_recv() {
            Ok(bytes) => {
                self.incoming.fetch_sub(bytes.len(), Ordering::AcqRel);

                Ok(Some(bytes))
            }
            Err(mpsc::TryRecvError::Empty) => Ok(None),
            Err(mpsc::TryRecvError::Disconnected) => Err(Error::Closed),
        }
    }

    fn flush(&mut self) -> Result<()> {
        Ok(())
    }
}
