use priority_queue::PriorityQueue;
use std::{
    cmp::{Ordering, Reverse},
    collections::HashMap,
};
use tokio::sync::{
    mpsc::{self},
    oneshot::{self, channel},
};

const SCHEDULED_PRIORITY_LEVELS: usize = 10;
const PRIORITY_LEVEL_WIDTH: usize = 8;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PriorityLevelEntry {
    Occupied((u64, u64)),
    Empty,
}

impl PartialOrd for PriorityLevelEntry {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for PriorityLevelEntry {
    fn cmp(&self, other: &Self) -> Ordering {
        match (self, other) {
            (Self::Empty, Self::Empty) => Ordering::Equal,
            (Self::Occupied(_), Self::Empty) => Ordering::Less,
            (Self::Empty, Self::Occupied(_)) => Ordering::Greater,
            (Self::Occupied((_, x)), Self::Occupied((_, y))) => x.cmp(y).reverse(),
        }
    }
}

struct WorkloadManager {
    rx: mpsc::Receiver<WorkloadManagerMessage>,
    queue: PriorityQueue<(u64, u64), Reverse<u64>>,
    senders: HashMap<u64, oneshot::Sender<()>>,
    priority_levels: [PriorityLevelEntry; SCHEDULED_PRIORITY_LEVELS],
}

impl WorkloadManager {
    fn sort_priority_levels(&mut self) {
        self.priority_levels.sort();
    }

    fn find_empty_entry_positions(&mut self) -> Vec<usize> {
        use PriorityLevelEntry::*;
        self.priority_levels
            .iter()
            .enumerate()
            .filter(|entry| *entry.1 == Empty)
            .map(|entry| entry.0)
            .collect()
    }

    fn find_message_position(&mut self, id: u64) -> Option<usize> {
        use PriorityLevelEntry::*;
        self.priority_levels.iter().position(|entry| {
            if let Occupied((existing_id, _)) = entry {
                return id == *existing_id;
            }
            false
        })
    }

    fn find_message(&mut self, id: u64) -> Option<&mut PriorityLevelEntry> {
        if let Some(position) = self.find_message_position(id) {
            return self.priority_levels.get_mut(position);
        }
        return None;
    }

    fn update_message(&mut self, id: u64, remaining_datagrams: u64) -> Option<u8> {
        use PriorityLevelEntry::*;
        if let Some(entry) = self.find_message(id) {
            *entry = Occupied((id, remaining_datagrams));
            self.sort_priority_levels();
        }
        if let Some(position) = self.find_message_position(id) {
            return Some(position as u8 * PRIORITY_LEVEL_WIDTH as u8);
        }
        None
    }

    fn handle_register_message(
        &mut self,
        id: u64,
        remaining_datagrams: u64,
        tx: oneshot::Sender<()>,
    ) {
        self.queue
            .push((id, remaining_datagrams), Reverse(remaining_datagrams));
        self.senders.insert(id, tx);
        self.try_activate_messages();
    }

    fn handle_get_priority(&mut self, id: u64, remaining_datagrams: u64, tx: oneshot::Sender<u8>) {
        if let Some(priority) = self.update_message(id, remaining_datagrams) {
            tx.send(priority).unwrap();
        }
    }

    fn handle_unregister_message(&mut self, id: u64) {
        use PriorityLevelEntry::*;
        if let Some(entry) = self.find_message(id) {
            *entry = Empty;
            self.try_activate_messages();
        }
    }

    fn try_activate_messages(&mut self) {
        use PriorityLevelEntry::*;
        let empty_positions = self.find_empty_entry_positions();
        for position in empty_positions {
            if let Some((entry, _)) = self.queue.pop() {
                self.priority_levels[position] = Occupied(entry);
                let tx = self.senders.remove(&entry.0).unwrap();
                tx.send(()).unwrap();
            }
        }
        self.sort_priority_levels();
    }

    fn handle_workload_manager_message(
        &mut self,
        workload_manager_message: WorkloadManagerMessage,
    ) {
        use WorkloadManagerMessage::*;
        match workload_manager_message {
            RegisterMessage((id, remaining_datagrams, tx)) => {
                self.handle_register_message(id, remaining_datagrams, tx);
            }
            GetPriority((id, remaining_datagrams, tx)) => {
                self.handle_get_priority(id, remaining_datagrams, tx);
            }
            UnregisterMessage(id) => {
                self.handle_unregister_message(id);
            }
        };
        println!("WORKLOAD MANAGER: {:?}", self.priority_levels);
    }
}

pub enum WorkloadManagerMessage {
    RegisterMessage((u64, u64, oneshot::Sender<()>)),
    GetPriority((u64, u64, oneshot::Sender<u8>)),
    UnregisterMessage(u64),
}

fn run_workload_manager(mut workload_manager: WorkloadManager) {
    while let Some(workload_manager_message) = workload_manager.rx.blocking_recv() {
        workload_manager.handle_workload_manager_message(workload_manager_message);
    }
}

#[derive(Clone)]
pub struct WorkloadManagerHandle {
    pub tx: mpsc::Sender<WorkloadManagerMessage>,
}

impl WorkloadManagerHandle {
    pub fn new() -> Self {
        let (tx, rx) = mpsc::channel::<WorkloadManagerMessage>(1000);
        use PriorityLevelEntry::*;
        let workload_manager = WorkloadManager {
            rx,
            queue: PriorityQueue::new(),
            senders: HashMap::new(),
            priority_levels: [Empty; SCHEDULED_PRIORITY_LEVELS],
        };
        tokio::task::spawn_blocking(move || run_workload_manager(workload_manager));
        Self { tx }
    }

    pub async fn register_message(&self, message_id: u64, remaining_bytes: u64) {
        use WorkloadManagerMessage::*;
        let (tx, rx) = channel::<()>();
        let workload_manager_message = RegisterMessage((message_id, remaining_bytes, tx));
        self.tx.send(workload_manager_message).await.unwrap();
        rx.await.unwrap();
    }

    pub async fn get_priority(&self, message_id: u64, remaining_datagrams: u64) -> u8 {
        use WorkloadManagerMessage::*;
        let (tx, rx) = channel::<u8>();
        let workload_manager_message = GetPriority((message_id, remaining_datagrams, tx));
        self.tx.send(workload_manager_message).await.unwrap();
        rx.await.unwrap()
    }

    pub async fn unregister_message(&self, message_id: u64) {
        use WorkloadManagerMessage::*;
        let workload_manager_message = UnregisterMessage(message_id);
        self.tx.send(workload_manager_message).await.unwrap();
    }
}
