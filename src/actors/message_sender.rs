use super::application::ApplicationHandle;
use super::datagram_sender::DatagramSenderHandle;
use super::priority_manager::PriorityManagerHandle;
use super::workload_manager::{self, WorkloadManagerHandle};
use crate::constants::UNSCHEDULED_HOMA_DATAGRAM_LIMIT;
use crate::models::{datagram::HomaDatagram, message::HomaMessage};
use rand::Rng;
use std::ops::Range;
use tokio::select;
use tokio::sync::mpsc::{channel, Receiver, Sender};
use tokio::task::JoinHandle;
use tokio::time::{sleep, Duration};

struct MessageSender {
    rx: Receiver<HomaDatagram>,
    message: HomaMessage,
    datagrams: Vec<HomaDatagram>,
    application_handle: ApplicationHandle,
    priority_manager_handle: PriorityManagerHandle,
    workload_manager_handle: WorkloadManagerHandle,
    datagram_sender_handle: DatagramSenderHandle,
    unscheduled_priority: u8,
    workload: Vec<u64>,
}

impl MessageSender {
    async fn handle_datagram(&self, datagram: HomaDatagram) {
        use crate::models::datagram::HomaDatagramType::*;
        match datagram.datagram_type {
            Grant => self.handle_grant(datagram).await,
            Resend => self.handle_resend(datagram).await,
            _ => (),
        }
    }

    async fn handle_grant(&self, grant: HomaDatagram) {
        if let Some(data) = self.datagrams.get(grant.sequence_number as usize) {
            data.to_owned().workload = self.workload.clone();
            let packet = data.to_ipv4(grant.priority);
            self.datagram_sender_handle
                .send(packet)
                .await
                .expect("MessageSender -> DatagramSender failed");
        }
    }

    async fn handle_resend(&self, resend_datagram: HomaDatagram) {
        if let Some(next_datagram) = self.datagrams.get(resend_datagram.sequence_number as usize) {
            next_datagram.to_owned().workload = self.workload.clone();
            let packet = next_datagram.to_ipv4(resend_datagram.priority);
            self.datagram_sender_handle
                .send(packet)
                .await
                .expect("MessageSender -> DatagramSender failed");
        }
    }

    fn check_message(&self, datagram: &HomaDatagram) -> bool {
        if datagram.sequence_number == self.datagrams.len() as u64 {
            return true;
        }
        false
    }

    async fn send_datagram_slice(&mut self, start: usize, end: usize) {
        for i in start..end {
            if let Some(datagram) = self.datagrams.get(i) {
                let mut datagram = datagram.to_owned();
                datagram.workload = self.workload.clone();
                datagram.priority = self.unscheduled_priority;
                let packet = datagram.to_ipv4(self.unscheduled_priority);
                self.datagram_sender_handle
                    .send(packet)
                    .await
                    .expect("MessageSender -> DatagramSender failed");
            }
        }
    }

    async fn send_unscheduled_datagrams(&mut self) -> Option<HomaDatagram> {
        self.send_datagram_slice(0, UNSCHEDULED_HOMA_DATAGRAM_LIMIT)
            .await;
        let mut resend_counter = 0;
        loop {
            // let resend_timeout_millis = rand::thread_rng().gen_range::<u64, Range<u64>>(500..2000);
            let resend_timeout_millis = rand::thread_rng().gen_range::<u64, Range<u64>>(20..40);
            select! {
                _ = sleep(Duration::from_millis(resend_timeout_millis)) => {
                    if resend_counter == 5 {
                        return None
                    }
                    self.send_datagram_slice(0, UNSCHEDULED_HOMA_DATAGRAM_LIMIT)
                        .await;
                    resend_counter += 1;
                }
                Some(datagram) = self.rx.recv() => {
                    self.priority_manager_handle
                        .put_unscheduled_priority_level_partitions(
                            datagram.source_address.clone(),
                            datagram.workload.clone(),
                        )
                        .await;
                    return Some(datagram);
                }
            }
        }
    }

    async fn send_requested_datagrams(&mut self, datagram: HomaDatagram) {
        self.handle_datagram(datagram).await;
        loop {
            select! {
                _ = sleep(Duration::from_millis(10000)) => {
                    return;
                }
                Some(datagram) = self.rx.recv() => {
                    self.priority_manager_handle
                        .put_unscheduled_priority_level_partitions(
                            datagram.source_address.clone(),
                            datagram.workload.clone(),
                        )
                        .await;
                    if self.check_message(&datagram) {
                        return
                    }
                    self.handle_datagram(datagram).await;
                }
            }
        }
    }

    async fn complete(&mut self) {
        use crate::actors::application::ApplicationMessage::*;
        self.rx.close();
        let _ = self
            .application_handle
            .send(FromMessageSender(self.message.id))
            .await;
    }
}

async fn run_message_sender(mut message_sender: MessageSender) {
    message_sender.unscheduled_priority = message_sender
        .priority_manager_handle
        .get_unscheduled_priority(
            message_sender.message.destination_address.clone(),
            message_sender.message.content.len() as u64,
        )
        .await;
    message_sender.workload = message_sender
        .workload_manager_handle
        .get_priority_level_partitions()
        .await
        .expect("MessageSender -> WorkloadManager failed");
    if let Some(datagram) = message_sender.send_unscheduled_datagrams().await {
        if !message_sender.check_message(&datagram) {
            message_sender.send_requested_datagrams(datagram).await;
        }
    }
    message_sender.complete().await;
}

#[derive(Clone)]
pub struct MessageSenderHandle {
    pub tx: Sender<HomaDatagram>,
}

impl MessageSenderHandle {
    pub fn new(
        message: HomaMessage,
        application_handle: ApplicationHandle,
        priority_manager_handle: PriorityManagerHandle,
        workload_manager_handle: WorkloadManagerHandle,
        datagram_sender_handle: DatagramSenderHandle,
    ) -> (Self, JoinHandle<()>) {
        let (tx, rx) = channel::<HomaDatagram>(100000);
        let datagrams = message.split();
        let message_sender = MessageSender {
            rx,
            message,
            datagrams,
            application_handle,
            priority_manager_handle,
            workload_manager_handle,
            datagram_sender_handle,
            unscheduled_priority: 0,
            workload: Vec::new(),
        };
        let join_handle = tokio::spawn(run_message_sender(message_sender));
        (Self { tx }, join_handle)
    }
}
