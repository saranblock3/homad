use std::collections::HashMap;
use tokio::sync::mpsc::{channel, Receiver, Sender};

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
                    tx.blocking_send(Some(*priority)).unwrap();
                } else {
                    tx.blocking_send(None).unwrap();
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
        tx: Sender<Option<u8>>,
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
}
