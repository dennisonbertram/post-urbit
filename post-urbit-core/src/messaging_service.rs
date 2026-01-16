use std::sync::{Arc, Mutex};

use tokio::sync::mpsc;

use crate::error::{PostUrbitError, Result};

pub trait MessagingTransport: Send + Sync {
    fn send(&self, peer: &str, payload: Vec<u8>) -> Result<()>;
    fn subscribe(&self) -> Result<mpsc::Receiver<Vec<u8>>>;
}

#[derive(Clone)]
pub struct MessagingService<T: MessagingTransport> {
    transport: Arc<T>,
}

impl<T: MessagingTransport> MessagingService<T> {
    pub fn new(transport: T) -> Self {
        Self {
            transport: Arc::new(transport),
        }
    }

    pub fn send_message(&self, peer: &str, payload: Vec<u8>) -> Result<()> {
        self.transport.send(peer, payload)
    }

    pub fn subscribe(&self) -> Result<mpsc::Receiver<Vec<u8>>> {
        self.transport.subscribe()
    }
}

pub struct TransportStub {
    sender: mpsc::Sender<Vec<u8>>,
    receiver: Arc<Mutex<Option<mpsc::Receiver<Vec<u8>>>>>,
}

impl TransportStub {
    pub fn new() -> Self {
        let (sender, receiver) = mpsc::channel(8);
        Self {
            sender,
            receiver: Arc::new(Mutex::new(Some(receiver))),
        }
    }
}

impl MessagingTransport for TransportStub {
    fn send(&self, _peer: &str, payload: Vec<u8>) -> Result<()> {
        self.sender
            .try_send(payload)
            .map_err(|_| PostUrbitError::InvalidInput("transport send"))
    }

    fn subscribe(&self) -> Result<mpsc::Receiver<Vec<u8>>> {
        let mut guard = self.receiver.lock().unwrap();
        guard
            .take()
            .ok_or(PostUrbitError::InvalidInput("transport subscribed"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn messaging_service_send_receive() {
        let transport = TransportStub::new();
        let service = MessagingService::new(transport);
        let mut receiver = service.subscribe().unwrap();

        service.send_message("peer", b"hello".to_vec()).unwrap();
        let received = receiver.recv().await.unwrap();
        assert_eq!(received, b"hello".to_vec());
    }
}
