use crate::constants::{HOMA_DATAGRAM_PAYLOAD_LENGTH, UNSCHEDULED_HOMA_DATAGRAM_LIMIT};
use crate::utils::quantile;
use std::collections::VecDeque;
use std::thread;
use tokio::sync::mpsc::error::SendError;
use tokio::sync::mpsc::{channel, Receiver, Sender};
use tokio::sync::oneshot;

const UNSCHEDULED_PRIORITY_LEVELS: usize = 6;

struct WorkloadManager {
    message_lengths: Vec<u64>,
    priority_level_partitions: Vec<u64>,
    rx: Receiver<WorkloadManagerMessage>,
}

// The Application should register each new incoming message with the WorkloadManager.
// The WorkloadManager will then calculate the appropriate partitions to be piggybacked
// by each MessageSender/MessageReceiver. The WorkloadManager will also be queried by the
// Application for the appropriate priority to send unscheduled datagrams to a remote host.

impl WorkloadManager {
    fn handle_workload_manager_message(
        &mut self,
        workload_manager_message: WorkloadManagerMessage,
    ) {
        use WorkloadManagerMessage::*;
        match workload_manager_message {
            PutMessageLength(message_length) => self.handle_put_message_length(message_length),
            GetPriorityLevelPartitions(return_chan) => {
                self.handle_get_priority_level_partitions(return_chan)
            }
        }
    }

    fn handle_put_message_length(&mut self, message_length: u64) {
        let i = self
            .message_lengths
            .binary_search(&message_length)
            .unwrap_or_else(|j| j);
        self.message_lengths.insert(i, message_length);
        self.calculate_priority_level_partitions();
    }

    fn handle_get_priority_level_partitions(&self, return_chan: oneshot::Sender<Vec<u64>>) {
        let _ = return_chan.send(self.priority_level_partitions.clone());
    }

    fn calculate_priority_level_partitions(&mut self) {
        self.priority_level_partitions = vec![];
        if self.message_lengths.len() < 100 {
            let rtt_bytes = UNSCHEDULED_HOMA_DATAGRAM_LIMIT * HOMA_DATAGRAM_PAYLOAD_LENGTH as usize;
            let priority_level_width = rtt_bytes / UNSCHEDULED_PRIORITY_LEVELS;
            for i in 1..UNSCHEDULED_PRIORITY_LEVELS {
                let partition = (i * priority_level_width) as u64;
                self.priority_level_partitions.push(partition);
            }
        } else {
            for i in 1..UNSCHEDULED_PRIORITY_LEVELS {
                let q = (1. / UNSCHEDULED_PRIORITY_LEVELS as f32) * i as f32;
                self.priority_level_partitions
                    .push(quantile(&self.message_lengths, q));
            }
        }
    }
}

fn run_workload_manager(mut workload_manager: WorkloadManager) {
    workload_manager.calculate_priority_level_partitions();
    while let Some(workload_manager_message) = workload_manager.rx.blocking_recv() {
        workload_manager.handle_workload_manager_message(workload_manager_message);
    }
}

pub enum WorkloadManagerMessage {
    PutMessageLength(u64),
    GetPriorityLevelPartitions(oneshot::Sender<Vec<u64>>),
}

#[derive(Clone)]
pub struct WorkloadManagerHandle {
    pub tx: Sender<WorkloadManagerMessage>,
}

impl WorkloadManagerHandle {
    pub fn new() -> Self {
        let (tx, rx) = channel::<WorkloadManagerMessage>(10000);
        let workload_manager = WorkloadManager {
            rx,
            message_lengths: Vec::new(),
            priority_level_partitions: Vec::new(),
        };
        thread::spawn(move || run_workload_manager(workload_manager));
        Self { tx }
    }

    pub async fn put_message_length(&self, message_length: u64) -> Result<(), ()> {
        use WorkloadManagerMessage::*;
        let workload_manager_message = PutMessageLength(message_length);
        self.tx.send(workload_manager_message).await.map_err(|_| ())
    }

    pub async fn get_priority_level_partitions(&self) -> Result<Vec<u64>, ()> {
        let (tx, rx) = oneshot::channel::<Vec<u64>>();
        use WorkloadManagerMessage::*;
        let workload_manager_message = GetPriorityLevelPartitions(tx);
        self.tx
            .send(workload_manager_message)
            .await
            .map_err(|_| ())?;
        rx.await.map_err(|_| ())
    }
}
