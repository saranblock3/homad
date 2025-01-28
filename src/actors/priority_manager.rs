use std::collections::HashMap;
use tokio::sync::mpsc::{channel, Receiver, Sender};
use tokio::sync::oneshot;

struct PriorityManager {
    receiver_priorities: HashMap<[u8; 4], u8>,
    rx: Receiver<PriorityManagerMessage>,
}

impl PriorityManager {
    fn handle_priority_manager_message(
        &mut self,
        priority_manager_message: PriorityManagerMessage,
    ) {
        use PriorityManagerMessage::*;
        match priority_manager_message {
            Put { address, priority } => {
                self.receiver_priorities.insert(address, priority);
            }
            Get { address, tx } => {
                if let Some(priority) = self.receiver_priorities.get(&address) {
                    tx.send(Some(*priority)).unwrap();
                } else {
                    tx.send(None).unwrap();
                }
            }
        }
    }
}

fn run_priority_manager(mut priority_manager: PriorityManager) {
    while let Some(priority_manager_message) = priority_manager.rx.blocking_recv() {
        priority_manager.handle_priority_manager_message(priority_manager_message);
    }
}

pub enum PriorityManagerMessage {
    Put {
        address: [u8; 4],
        priority: u8,
    },
    Get {
        address: [u8; 4],
        tx: oneshot::Sender<Option<u8>>,
    },
}

#[derive(Clone)]
pub struct PriorityManagerHandle {
    pub tx: Sender<PriorityManagerMessage>,
}

impl PriorityManagerHandle {
    pub fn new() -> Self {
        let (tx, rx) = channel::<PriorityManagerMessage>(1000);
        let priority_manager = PriorityManager {
            receiver_priorities: HashMap::new(),
            rx,
        };
        tokio::task::spawn_blocking(move || {
            run_priority_manager(priority_manager);
        });
        Self { tx }
    }

    pub async fn put_priority(&self, address: [u8; 4], priority: u8) {
        use PriorityManagerMessage::*;
        let priority_manager_message = Put { address, priority };
        self.tx.send(priority_manager_message).await.unwrap();
    }

    pub async fn get_priority(&self, address: [u8; 4]) -> Option<u8> {
        use PriorityManagerMessage::*;
        let (tx, rx) = oneshot::channel();
        let priority_manager_message = Get { address, tx };
        self.tx.send(priority_manager_message).await.unwrap();
        rx.await.unwrap()
    }
}
