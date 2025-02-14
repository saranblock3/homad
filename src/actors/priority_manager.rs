use priority_queue::PriorityQueue;
use std::{
    cmp::{Ordering, Reverse},
    collections::HashMap,
};
use tokio::sync::{
    mpsc::{self},
    oneshot::{self, channel},
};

use crate::constants::{HOMA_DATAGRAM_PAYLOAD_LENGTH, UNSCHEDULED_HOMA_DATAGRAM_LIMIT};

const SCHEDULED_PRIORITY_LEVELS: usize = 6;
const PRIORITY_LEVEL_WIDTH: usize = 8;
const UNSCHEDULED_PRIORITY_LEVELS: usize = 6;

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

struct PriorityManager {
    rx: mpsc::Receiver<PriorityManagerMessage>,
    queue: PriorityQueue<(u64, u64), Reverse<u64>>,
    senders: HashMap<u64, oneshot::Sender<()>>,
    scheduled_priority_levels: [PriorityLevelEntry; SCHEDULED_PRIORITY_LEVELS],
    unscheduled_priority_levels: HashMap<[u8; 4], Vec<u64>>,
}

impl PriorityManager {
    fn sort_priority_levels(&mut self) {
        self.scheduled_priority_levels.sort();
    }

    fn find_empty_entry_positions(&mut self) -> Vec<usize> {
        use PriorityLevelEntry::*;
        self.scheduled_priority_levels
            .iter()
            .enumerate()
            .filter(|entry| *entry.1 == Empty)
            .map(|entry| entry.0)
            .collect()
    }

    fn find_message_position(&mut self, id: u64) -> Option<usize> {
        use PriorityLevelEntry::*;
        self.scheduled_priority_levels.iter().position(|entry| {
            if let Occupied((existing_id, _)) = entry {
                return id == *existing_id;
            }
            false
        })
    }

    fn find_message(&mut self, id: u64) -> Option<&mut PriorityLevelEntry> {
        if let Some(position) = self.find_message_position(id) {
            return self.scheduled_priority_levels.get_mut(position);
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

    fn handle_register_scheduled_message(
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

    fn handle_get_scheduled_priority(
        &mut self,
        id: u64,
        remaining_datagrams: u64,
        tx: oneshot::Sender<u8>,
    ) {
        if let Some(priority) = self.update_message(id, remaining_datagrams) {
            tx.send(priority).unwrap();
        }
    }

    fn handle_unregister_scheduled_message(&mut self, id: u64) {
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
                self.scheduled_priority_levels[position] = Occupied(entry);
                let tx = self.senders.remove(&entry.0).unwrap();
                tx.send(()).unwrap();
            }
        }
        self.sort_priority_levels();
    }

    fn handle_get_unscheduled_priority(
        &self,
        address: [u8; 4],
        message_length: u64,
        tx: oneshot::Sender<u8>,
    ) {
        match self.unscheduled_priority_levels.get(&address) {
            Some(priority_level_partitions) => {
                for (i, partition) in priority_level_partitions.iter().enumerate() {
                    if message_length <= *partition {
                        tx.send((56 - i * 8) as u8);
                        return;
                    }
                }
                let _ = tx.send((64 - priority_level_partitions.len() * 8) as u8);
            }
            None => {
                let rtt_bytes =
                    UNSCHEDULED_HOMA_DATAGRAM_LIMIT * HOMA_DATAGRAM_PAYLOAD_LENGTH as usize;
                let priority_level_width = rtt_bytes / UNSCHEDULED_PRIORITY_LEVELS;
                for i in 1..UNSCHEDULED_PRIORITY_LEVELS {
                    let partition = (i * priority_level_width) as u64;
                    if message_length <= partition {
                        let _ = tx.send((64 - i * 8) as u8);
                        return;
                    }
                }
                let _ = tx.send((64 - UNSCHEDULED_PRIORITY_LEVELS * 8) as u8);
            }
        }
    }

    fn handle_put_priority_level_partions(
        &mut self,
        address: [u8; 4],
        priority_level_partitions: Vec<u64>,
    ) {
        self.unscheduled_priority_levels
            .insert(address, priority_level_partitions);
    }

    fn handle_priority_manager_message(
        &mut self,
        priority_manager_message: PriorityManagerMessage,
    ) {
        use PriorityManagerMessage::*;
        match priority_manager_message {
            RegisterScheduledMessage((id, remaining_datagrams, tx)) => {
                self.handle_register_scheduled_message(id, remaining_datagrams, tx);
            }
            GetScheduledPriority((id, remaining_datagrams, tx)) => {
                self.handle_get_scheduled_priority(id, remaining_datagrams, tx);
            }
            UnregisterScheduledMessage(id) => {
                self.handle_unregister_scheduled_message(id);
            }
            GetUnscheduledPriority((address, message_length, tx)) => {
                self.handle_get_unscheduled_priority(address, message_length, tx)
            }
            PutUnscheduledPriorityLevelPartitions((address, priority_level_partitions)) => {
                self.handle_put_priority_level_partions(address, priority_level_partitions);
            }
        };
        println!("PRIORITY MANAGER: {:?}", self.scheduled_priority_levels);
    }
}

pub enum PriorityManagerMessage {
    RegisterScheduledMessage((u64, u64, oneshot::Sender<()>)),
    GetScheduledPriority((u64, u64, oneshot::Sender<u8>)),
    UnregisterScheduledMessage(u64),
    GetUnscheduledPriority(([u8; 4], u64, oneshot::Sender<u8>)),
    PutUnscheduledPriorityLevelPartitions(([u8; 4], Vec<u64>)),
}

fn run_priority_manager(mut priority_manager: PriorityManager) {
    while let Some(priority_manager_message) = priority_manager.rx.blocking_recv() {
        priority_manager.handle_priority_manager_message(priority_manager_message);
    }
}

#[derive(Clone)]
pub struct PriorityManagerHandle {
    pub tx: mpsc::Sender<PriorityManagerMessage>,
}

impl PriorityManagerHandle {
    pub fn new() -> Self {
        let (tx, rx) = mpsc::channel::<PriorityManagerMessage>(1000);
        use PriorityLevelEntry::*;
        let priority_manager = PriorityManager {
            rx,
            queue: PriorityQueue::new(),
            senders: HashMap::new(),
            scheduled_priority_levels: [Empty; SCHEDULED_PRIORITY_LEVELS],
            unscheduled_priority_levels: HashMap::new(),
        };
        tokio::task::spawn_blocking(move || run_priority_manager(priority_manager));
        Self { tx }
    }

    pub async fn register_scheduled_message(&self, message_id: u64, remaining_bytes: u64) {
        use PriorityManagerMessage::*;
        let (tx, rx) = channel::<()>();
        let priority_manager_message = RegisterScheduledMessage((message_id, remaining_bytes, tx));
        self.tx.send(priority_manager_message).await.unwrap();
        rx.await.unwrap();
    }

    pub async fn get_scheduled_priority(&self, message_id: u64, remaining_datagrams: u64) -> u8 {
        use PriorityManagerMessage::*;
        let (tx, rx) = channel::<u8>();
        let priority_manager_message = GetScheduledPriority((message_id, remaining_datagrams, tx));
        self.tx.send(priority_manager_message).await.unwrap();
        rx.await.unwrap()
    }

    pub async fn unregister_scheduled_message(&self, message_id: u64) {
        use PriorityManagerMessage::*;
        let priority_manager_message = UnregisterScheduledMessage(message_id);
        self.tx.send(priority_manager_message).await.unwrap();
    }

    pub async fn get_unscheduled_priority(&self, address: [u8; 4], message_length: u64) -> u8 {
        use PriorityManagerMessage::*;
        let (tx, rx) = oneshot::channel();
        let priority_manager_message = GetUnscheduledPriority((address, message_length, tx));
        self.tx.send(priority_manager_message).await.unwrap();
        rx.await.unwrap()
    }

    pub async fn put_unscheduled_priority_level_partitions(
        &self,
        address: [u8; 4],
        priority_level_partitions: Vec<u64>,
    ) {
        use PriorityManagerMessage::*;
        let priority_manager_message =
            PutUnscheduledPriorityLevelPartitions((address, priority_level_partitions));
        self.tx.send(priority_manager_message).await.unwrap();
    }
}
