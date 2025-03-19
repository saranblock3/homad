use crate::config::CONFIG;
use crate::config::CONST;
use priority_queue::PriorityQueue;
use std::cmp::Ordering;
use std::cmp::Reverse;
use std::collections::HashMap;
use std::net::Ipv4Addr;
use tokio::sync::mpsc;
use tokio::sync::oneshot;
use tokio::sync::oneshot::channel;

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
    scheduled_priority_levels: [PriorityLevelEntry; CONST::SCHEDULED_PRIORITY_LEVELS],
    unscheduled_priority_partitions:
        HashMap<Ipv4Addr, [u64; CONST::UNSCHEDULED_PRIORITY_PARTITIONS]>,
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
            return Some(position as u8 * CONST::PRIORITY_LEVEL_WIDTH as u8);
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
            let _ = tx.send(priority);
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
                if let Some(tx) = self.senders.remove(&entry.0) {
                    self.scheduled_priority_levels[position] = Occupied(entry);
                    let _ = tx.send(());
                }
            }
        }
        self.sort_priority_levels();
    }

    fn handle_get_unscheduled_priority(
        &self,
        address: Ipv4Addr,
        message_length: u64,
        tx: oneshot::Sender<u8>,
    ) {
        match self.unscheduled_priority_partitions.get(&address) {
            Some(priority_level_partitions) => {
                for (i, partition) in priority_level_partitions.iter().enumerate() {
                    if message_length <= *partition {
                        let _ = tx.send((56 - i * 8) as u8);
                        return;
                    }
                }
                let _ = tx.send((64 - priority_level_partitions.len() * 8) as u8);
            }
            _ => {
                let rtt_bytes =
                    CONFIG.UNSCHEDULED_DATAGRAM_LIMIT * CONFIG.DATAGRAM_PAYLOAD_LENGTH as usize;
                let priority_level_width = rtt_bytes / CONST::UNSCHEDULED_PRIORITY_LEVELS;
                for i in 1..CONST::UNSCHEDULED_PRIORITY_LEVELS {
                    let partition = (i * priority_level_width) as u64;
                    if message_length <= partition {
                        let _ = tx.send((64 - i * 8) as u8);
                        return;
                    }
                }
                let _ = tx.send((64 - CONST::UNSCHEDULED_PRIORITY_LEVELS * 8) as u8);
            }
        }
    }

    fn handle_put_priority_level_partions(
        &mut self,
        address: Ipv4Addr,
        priority_level_partitions: [u64; CONST::UNSCHEDULED_PRIORITY_PARTITIONS],
    ) {
        self.unscheduled_priority_partitions
            .insert(address, priority_level_partitions);
    }

    fn handle_priority_manager_message(
        &mut self,
        priority_manager_message: PriorityManagerMessage,
    ) {
        use PriorityManagerMessage::*;
        match priority_manager_message {
            RegisterScheduledMessage(id, remaining_datagrams, tx) => {
                self.handle_register_scheduled_message(id, remaining_datagrams, tx);
            }
            GetScheduledPriority(id, remaining_datagrams, tx) => {
                self.handle_get_scheduled_priority(id, remaining_datagrams, tx);
            }
            UnregisterScheduledMessage(id) => {
                self.handle_unregister_scheduled_message(id);
            }
            GetUnscheduledPriority(address, message_length, tx) => {
                self.handle_get_unscheduled_priority(address, message_length, tx)
            }
            PutUnscheduledPriorityLevelPartitions(address, priority_level_partitions) => {
                self.handle_put_priority_level_partions(address, priority_level_partitions);
            }
        };
    }
}

pub enum PriorityManagerMessage {
    RegisterScheduledMessage(u64, u64, oneshot::Sender<()>),
    GetScheduledPriority(u64, u64, oneshot::Sender<u8>),
    UnregisterScheduledMessage(u64),
    GetUnscheduledPriority(Ipv4Addr, u64, oneshot::Sender<u8>),
    PutUnscheduledPriorityLevelPartitions(Ipv4Addr, [u64; CONST::UNSCHEDULED_PRIORITY_PARTITIONS]),
}

fn run_priority_manager(mut priority_manager: PriorityManager) {
    while let Some(priority_manager_message) = priority_manager.rx.blocking_recv() {
        priority_manager.handle_priority_manager_message(priority_manager_message);
    }
}

#[derive(Clone)]
pub struct PriorityManagerHandle {
    tx: mpsc::Sender<PriorityManagerMessage>,
}

impl PriorityManagerHandle {
    pub fn new() -> Self {
        let (tx, rx) = mpsc::channel::<PriorityManagerMessage>(1000);
        use PriorityLevelEntry::*;
        let priority_manager = PriorityManager {
            rx,
            queue: PriorityQueue::new(),
            senders: HashMap::new(),
            scheduled_priority_levels: [Empty; CONST::SCHEDULED_PRIORITY_LEVELS],
            unscheduled_priority_partitions: HashMap::new(),
        };
        tokio::task::spawn_blocking(move || run_priority_manager(priority_manager));
        Self { tx }
    }

    pub async fn register_scheduled_message(&self, message_id: u64, remaining_datagrams: u64) {
        use PriorityManagerMessage::*;
        let (tx, rx) = channel::<()>();
        let priority_manager_message =
            RegisterScheduledMessage(message_id, remaining_datagrams, tx);
        let _ = self.tx.send(priority_manager_message).await;
        let _ = rx.await;
    }

    pub async fn get_scheduled_priority(&self, message_id: u64, remaining_datagrams: u64) -> u8 {
        use PriorityManagerMessage::*;
        let (tx, rx) = channel::<u8>();
        let priority_manager_message = GetScheduledPriority(message_id, remaining_datagrams, tx);
        let _ = self.tx.send(priority_manager_message).await;
        rx.await.unwrap()
    }

    pub async fn unregister_scheduled_message(&self, message_id: u64) {
        use PriorityManagerMessage::*;
        let priority_manager_message = UnregisterScheduledMessage(message_id);
        let _ = self.tx.send(priority_manager_message).await;
    }

    pub async fn get_unscheduled_priority(&self, address: Ipv4Addr, message_length: u64) -> u8 {
        use PriorityManagerMessage::*;
        let (tx, rx) = oneshot::channel();
        let priority_manager_message = GetUnscheduledPriority(address, message_length, tx);
        let _ = self.tx.send(priority_manager_message).await;
        rx.await.unwrap()
    }

    pub async fn put_unscheduled_priority_level_partitions(
        &self,
        address: Ipv4Addr,
        priority_level_partitions: [u64; CONST::UNSCHEDULED_PRIORITY_PARTITIONS],
    ) {
        use PriorityManagerMessage::*;
        let priority_manager_message =
            PutUnscheduledPriorityLevelPartitions(address, priority_level_partitions);
        let _ = self.tx.send(priority_manager_message).await;
    }
}
