/*
MessageSender actor

This actor sends out unscheduled datagrams and waits for a resend
or grant datagram

If the actor does not receive a resend or grant within a timeout,
the actor resends all unscheduled datagrams

The actor then send any requested datagrams specified by the resends or grants
*/
use crate::components::application::ApplicationHandle;
use crate::components::datagram_sender::DatagramSenderHandle;
use crate::components::priority_manager::PriorityManagerHandle;
use crate::components::workload_manager::WorkloadManagerHandle;
use crate::config::CONFIG;
use crate::config::CONST;
use crate::models::datagram::HomaDatagram;
use crate::models::message::HomaMessage;
use crate::utils::fuzz_timeout;
use std::net::Ipv4Addr;
use tokio::select;
use tokio::sync::mpsc::{channel, Receiver, Sender};
use tokio::task::JoinHandle;
use tokio::time::sleep;
use tokio::time::Duration;

struct MessageSender {
    message_id: u64,
    rx: Receiver<HomaDatagram>,

    source_address: Ipv4Addr,
    destination_address: Ipv4Addr,

    #[allow(unused)]
    source_id: u32,
    #[allow(unused)]
    destination_id: u32,
    content_length: u64,

    // Datagrams created by splitting the HomaMessage content
    datagrams: Vec<HomaDatagram>,

    // Priority to send unscheduled HomaDatagrams with
    unscheduled_priority: u8,
    // Workload of the current host to inform the remote host of
    workload: [u64; CONST::UNSCHEDULED_PRIORITY_PARTITIONS],

    // Actor handles to contact other relevant actors
    application_handle: ApplicationHandle,
    datagram_sender_handle: DatagramSenderHandle,
    priority_manager_handle: PriorityManagerHandle,
    workload_manager_handle: WorkloadManagerHandle,
}

impl MessageSender {
    // Multiplex and handle grant or resend types
    async fn handle_grant_or_resend(&mut self, datagram: HomaDatagram) {
        use crate::models::datagram::HomaDatagramType::*;
        match datagram.datagram_type {
            Grant => self.handle_grant(datagram).await,
            Resend => self.handle_resend(datagram).await,
            _ => (),
        }
    }

    // Send datagram requested by the grant datagram
    async fn handle_grant(&mut self, grant: HomaDatagram) {
        let i = grant.sequence_number as usize;
        let priority = grant.priority;
        self.send_datagram(i, priority).await;
    }

    // Send datagram requested by the resend datagram
    async fn handle_resend(&mut self, resend: HomaDatagram) {
        let i = resend.sequence_number as usize;
        let priority = self.unscheduled_priority;
        self.send_datagram(i, priority).await;
    }

    // Check if sequence number in the grant is equal to the number of datagrams
    fn check_message_transmitted(&self, grant: &HomaDatagram) -> bool {
        if grant.sequence_number == self.datagrams.len() as u32 {
            return true;
        }
        false
    }

    // Send datagram at index i and with specified priority
    async fn send_datagram(&mut self, i: usize, priority: u8) {
        if let Some(datagram) = self.datagrams.get(i) {
            let mut datagram = datagram.to_owned();
            datagram.workload = self.workload.clone();
            let _ = datagram.checksum();
            let packet = datagram.to_ipv4(self.source_address, self.destination_address, priority);
            self.datagram_sender_handle
                .send(packet)
                .await
                .expect("MessageSender -> DatagramSender failed");
        }
    }

    // Send all datagrams starting at index start
    // and ending at index end (non-inclusive)
    async fn send_datagram_slice(&mut self, start: usize, end: usize, priority: u8) {
        for i in start..end {
            if let Some(datagram) = self.datagrams.get(i) {
                let mut datagram = datagram.to_owned();
                datagram.workload = self.workload.clone();
                datagram.priority = self.unscheduled_priority;
                let _ = datagram.checksum();
                let packet =
                    datagram.to_ipv4(self.source_address, self.destination_address, priority);
                self.datagram_sender_handle
                    .send(packet)
                    .await
                    .expect("MessageSender -> DatagramSender failed");
            }
        }
    }

    // Send unscheduled datagrams and continuosly resend after a timeout if a
    // resend or grant is not received, fail after resend count reached
    async fn send_unscheduled_datagrams(&mut self) -> Option<HomaDatagram> {
        self.send_datagram_slice(
            0,
            CONFIG.UNSCHEDULED_DATAGRAM_LIMIT,
            self.unscheduled_priority,
        )
        .await;
        let mut resend_counter = 0;
        loop {
            let timeout = fuzz_timeout(CONFIG.TIMEOUT);
            select! {
                _ = sleep(Duration::from_millis(timeout)) => {
                    if resend_counter == CONFIG.RESENDS {
                        return None
                    }
                    self.send_datagram_slice(0, CONFIG.UNSCHEDULED_DATAGRAM_LIMIT, self.unscheduled_priority)
                        .await;
                    resend_counter += 1;
                }
                Some(grant_or_resend) = self.rx.recv() => {
                    self.priority_manager_handle
                        .put_unscheduled_priority_level_partitions(
                            self.destination_address,
                            grant_or_resend.workload.clone(),
                        )
                        .await;
                    return Some(grant_or_resend);
                }
            }
        }
    }

    // Send requested datagrams until all datagrams are sent and all have been
    // granted, fail after a max timeout
    async fn send_requested_datagrams(&mut self, datagram: HomaDatagram) {
        self.handle_grant_or_resend(datagram).await;
        loop {
            select! {
                _ = sleep(Duration::from_millis(CONFIG.LARGE_TIMEOUT)) => {
                    return;
                }
                Some(datagram) = self.rx.recv() => {
                    self.priority_manager_handle
                        .put_unscheduled_priority_level_partitions(
                            self.destination_address,
                            datagram.workload.clone(),
                        )
                        .await;
                    if self.check_message_transmitted(&datagram) {
                        return
                    }
                    self.handle_grant_or_resend(datagram).await;
                }
            }
        }
    }

    // Close the receiving channel and inform the Application actor
    async fn complete(&mut self) {
        use crate::components::application::ApplicationMessage::*;
        self.rx.close();
        let _ = self
            .application_handle
            .send(FromMessageSender(self.message_id))
            .await;
    }
}

// Get unscheduled priority for remote host and workload of the current host,
// send all unscheduled datagrams and then send all scheduled datagrams if
// necessary
async fn run_message_sender(mut message_sender: MessageSender) {
    message_sender.unscheduled_priority = message_sender
        .priority_manager_handle
        .get_unscheduled_priority(
            message_sender.destination_address,
            message_sender.content_length,
        )
        .await;
    message_sender.workload = message_sender
        .workload_manager_handle
        .get_workload()
        .await
        .expect("MessageSender -> WorkloadManager failed");
    if let Some(grant_or_resend) = message_sender.send_unscheduled_datagrams().await {
        if !message_sender.check_message_transmitted(&grant_or_resend) {
            message_sender
                .send_requested_datagrams(grant_or_resend)
                .await;
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
        datagram_sender_handle: DatagramSenderHandle,
        priority_manager_handle: PriorityManagerHandle,
        workload_manager_handle: WorkloadManagerHandle,
    ) -> (Self, JoinHandle<()>) {
        let (tx, rx) = channel::<HomaDatagram>(1000);
        let source_address = Ipv4Addr::from(message.source_address);
        let destination_address = Ipv4Addr::from(message.destination_address);
        let datagrams = message.split();
        let message_sender = MessageSender {
            message_id: message.id,
            rx,

            source_address,
            destination_address,

            source_id: message.source_id,
            destination_id: message.destination_id,
            content_length: message.content.len() as u64,

            datagrams,

            unscheduled_priority: 0,
            workload: [0; CONST::UNSCHEDULED_PRIORITY_PARTITIONS],

            application_handle,
            datagram_sender_handle,
            priority_manager_handle,
            workload_manager_handle,
        };
        let join_handle = tokio::spawn(run_message_sender(message_sender));
        (Self { tx }, join_handle)
    }
}
