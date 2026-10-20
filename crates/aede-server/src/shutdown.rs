//! Terminal server cancellation, retained for subscribers created after shutdown.

use tokio::sync::{broadcast, watch};

#[derive(Clone)]
pub(crate) struct Sender(watch::Sender<bool>);

impl Default for Sender {
    fn default() -> Self {
        Self(watch::channel(false).0)
    }
}

impl Sender {
    pub(crate) fn subscribe(&self) -> Receiver {
        Receiver(self.0.subscribe())
    }

    pub(crate) fn send(&self, (): ()) -> Result<usize, broadcast::error::SendError<()>> {
        let receivers = self.0.receiver_count();
        // Unlike send(), send_replace() retains the terminal value even when
        // no listener exists yet. No server state ever resets this to false.
        self.0.send_replace(true);
        if receivers == 0 {
            Err(broadcast::error::SendError(()))
        } else {
            Ok(receivers)
        }
    }
}

pub(crate) struct Receiver(watch::Receiver<bool>);

impl Receiver {
    pub(crate) async fn recv(&mut self) -> Result<(), watch::error::RecvError> {
        loop {
            if *self.0.borrow_and_update() {
                return Ok(());
            }
            self.0.changed().await?;
        }
    }

    pub(crate) fn try_recv(&mut self) -> Result<(), broadcast::error::TryRecvError> {
        if *self.0.borrow() {
            Ok(())
        } else if self.0.has_changed().is_err() {
            Err(broadcast::error::TryRecvError::Closed)
        } else {
            Err(broadcast::error::TryRecvError::Empty)
        }
    }
}

#[cfg(test)]
#[path = "shutdown_tests.rs"]
mod tests;
