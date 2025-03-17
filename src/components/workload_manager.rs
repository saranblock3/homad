use crate::config::CONFIG;
use crate::config::CONST;
use crate::utils::quantile;
use tokio::sync::mpsc::{channel, Receiver, Sender};
use tokio::sync::oneshot;

struct WorkloadManager {
    message_lengths: Vec<u64>,
    workload: [u64; CONST::UNSCHEDULED_PRIORITY_PARTITIONS],
    rx: Receiver<WorkloadManagerMessage>,
}

impl WorkloadManager {
    fn handle_workload_manager_message(
        &mut self,
        workload_manager_message: WorkloadManagerMessage,
    ) {
        use WorkloadManagerMessage::*;
        match workload_manager_message {
            GetWorkload(return_chan) => self.handle_get_workload(return_chan),
            UpdateWorkload(message_length, return_chan) => {
                self.handle_update_workload(message_length, return_chan)
            }
        }
    }

    fn handle_get_workload(
        &self,
        return_chan: oneshot::Sender<[u64; CONST::UNSCHEDULED_PRIORITY_PARTITIONS]>,
    ) {
        let _ = return_chan.send(self.workload.clone());
    }

    fn handle_update_workload(
        &mut self,
        message_length: u64,
        return_chan: oneshot::Sender<[u64; CONST::UNSCHEDULED_PRIORITY_PARTITIONS]>,
    ) {
        let i = self
            .message_lengths
            .binary_search(&message_length)
            .unwrap_or_else(|j| j);
        self.message_lengths.insert(i, message_length);
        self.calculate_priority_level_partitions();
        let _ = return_chan.send(self.workload.clone());
    }

    fn calculate_priority_level_partitions(&mut self) {
        self.workload = [0; CONST::UNSCHEDULED_PRIORITY_PARTITIONS];
        if self.message_lengths.len() < CONST::MINIMUM_WORKLOAD_SAMPLE_SIZE {
            let rtt_bytes =
                CONFIG.UNSCHEDULED_DATAGRAM_LIMIT * CONFIG.DATAGRAM_PAYLOAD_LENGTH as usize;
            let priority_level_width = rtt_bytes / CONST::UNSCHEDULED_PRIORITY_LEVELS;
            for i in 1..CONST::UNSCHEDULED_PRIORITY_LEVELS {
                let partition = (i * priority_level_width) as u64;
                self.workload[i - 1] = partition;
            }
        } else {
            for i in 1..CONST::UNSCHEDULED_PRIORITY_LEVELS {
                let q = (1. / CONST::UNSCHEDULED_PRIORITY_LEVELS as f32) * i as f32;
                self.workload[i - 1] = quantile(&self.message_lengths, q);
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
    GetWorkload(oneshot::Sender<[u64; CONST::UNSCHEDULED_PRIORITY_PARTITIONS]>),
    UpdateWorkload(
        u64,
        oneshot::Sender<[u64; CONST::UNSCHEDULED_PRIORITY_PARTITIONS]>,
    ),
}

#[derive(Clone)]
pub struct WorkloadManagerHandle {
    tx: Sender<WorkloadManagerMessage>,
}

impl WorkloadManagerHandle {
    pub fn new() -> Self {
        let (tx, rx) = channel::<WorkloadManagerMessage>(1000);
        let workload_manager = WorkloadManager {
            rx,
            message_lengths: Vec::new(),
            workload: [0; CONST::UNSCHEDULED_PRIORITY_PARTITIONS],
        };
        tokio::task::spawn_blocking(move || run_workload_manager(workload_manager));
        Self { tx }
    }

    pub async fn get_workload(
        &self,
    ) -> Result<[u64; CONST::UNSCHEDULED_PRIORITY_PARTITIONS], String> {
        use WorkloadManagerMessage::*;
        let (tx, rx) = oneshot::channel::<[u64; CONST::UNSCHEDULED_PRIORITY_PARTITIONS]>();
        let workload_manager_message = GetWorkload(tx);
        self.tx
            .send(workload_manager_message)
            .await
            .map_err(|_| "WorkloadManager failed to receive get request")?;
        rx.await
            .map_err(|_| "WorkloadManager failed to send get response".to_string())
    }

    pub async fn update_workload(
        &self,
        message_length: u64,
    ) -> Result<[u64; CONST::UNSCHEDULED_PRIORITY_PARTITIONS], String> {
        use WorkloadManagerMessage::*;
        let (tx, rx) = oneshot::channel::<[u64; CONST::UNSCHEDULED_PRIORITY_PARTITIONS]>();
        let workload_manager_message = UpdateWorkload(message_length, tx);
        self.tx
            .send(workload_manager_message)
            .await
            .map_err(|_| "WorkloadManager failed to receive update request")?;
        rx.await
            .map_err(|_| "WorkloadManager failed to send update response".to_string())
    }
}
