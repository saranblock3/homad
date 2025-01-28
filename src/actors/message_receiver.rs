use std::cmp::min;

use super::{
    application::ApplicationHandle, datagram_sender::DatagramSenderHandle,
    priority_manager::PriorityManagerHandle, workload_manager::WorkloadManagerHandle,
};
use crate::{
    constants::{
        HOMA_DATAGRAM_PAYLOAD_LENGTH, MESSAGE_RECEIVER_TIMEOUT, UNSCHEDULED_HOMA_DATAGRAM_LIMIT,
    },
    models::{
        datagram::{HomaDatagram, HomaDatagramType},
        message::{HomaMessage, HomaMessageBuilder},
    },
};
use tokio::{
    select,
    sync::mpsc::{channel, Receiver, Sender},
    task::JoinHandle,
    time::{sleep, Duration},
};

enum UnscheduledState {
    Incomplete,
    Complete,
}

enum ScheduledState {
    Incomplete,
    Complete,
}

struct MessageReceiver {
    datagram_sender_handle: DatagramSenderHandle,
    application_handle: ApplicationHandle,
    priority_manager_handle: PriorityManagerHandle,
    workload_manager_handle: WorkloadManagerHandle,
    rx: Receiver<HomaDatagram>,

    message_id: u64,
    datagram: HomaDatagram,
    source_address: [u8; 4],
    destination_address: [u8; 4],
    source_id: u32,
    destination_id: u32,
    expected: u64,
    collected: u64,
    unscheduled_only: bool,
    datagrams: Vec<Option<HomaDatagram>>,
}

impl MessageReceiver {
    pub fn add_datagram(&mut self, datagram: HomaDatagram) -> u64 {
        if let Some(datagram_entry) = self.datagrams.get_mut(datagram.sequence_number as usize) {
            if datagram_entry.is_none() {
                self.collected += 1;
                *datagram_entry = Some(datagram);
            }
        }
        self.datagrams.len() as u64
    }

    pub async fn receive_unscheduled_datagrams(&mut self) -> UnscheduledState {
        use UnscheduledState::*;
        let mut resend_counter = 0;
        loop {
            if let Complete = self.check_message_unscheduled() {
                return Complete;
            }
            let resend_timeout = sleep(Duration::from_millis(200));
            select! {
                _ = resend_timeout => {
                    if resend_counter == 5 {
                        return Incomplete;
                    }
                    self.request_resend_unscheduled_datagrams().await;
                    resend_counter += 1
                }
                Some(datagram) = self.rx.recv() => {
                    println!(
                        "MESSAGE RECEIVER (ID: {}) (ID: {}) RECEIVED UNSCHEDULED DATAGRAM FROM (ID: {})",
                        datagram.destination_id, datagram.message_id, datagram.source_id
                    );
                    self.add_datagram(datagram);
                }
            }
        }
    }

    pub async fn request_resend_unscheduled_datagrams(&mut self) {
        let mut resend = HomaDatagram::default();
        resend.message_id = self.message_id;
        resend.datagram_type = HomaDatagramType::Resend;
        resend.source_address = self.destination_address.clone();
        resend.destination_address = self.source_address.clone();
        resend.source_id = self.destination_id;
        resend.destination_id = self.source_id;
        for i in 0..UNSCHEDULED_HOMA_DATAGRAM_LIMIT {
            if let Some(None) = self.datagrams.get(i) {
                let mut resend = resend.clone();
                resend.sequence_number = i as u64;
                self.datagram_sender_handle
                    .tx
                    .send(resend.to_ipv4(64))
                    .await
                    .unwrap();
            }
        }
    }

    pub async fn receive_scheduled_datagrams(&mut self) -> ScheduledState {
        use ScheduledState::*;
        self.workload_manager_handle
            .register_message(self.message_id, self.expected - self.collected)
            .await;
        println!(
            "MESSAGE RECEIVER (ID: {}) (ID: {}) REGISTERED",
            self.datagram.destination_id, self.message_id
        );
        let priority = self
            .workload_manager_handle
            .get_priority(self.message_id, self.expected - self.collected)
            .await;
        println!(
            "MESSAGE RECEIVER (ID: {}) (ID: {}) RECEIVED FIRST PRIORITY",
            self.datagram.destination_id, self.message_id
        );
        self.grant(self.collected, priority).await;
        let mut resend_counter = 0;
        loop {
            select! {
                _ = sleep(Duration::from_millis(2000)) => {
                    if resend_counter == 5 {
                        println!(
                            "MESSAGE RECEIVER (ID: {}) (ID: {}) DIED",
                            self.datagram.destination_id, self.message_id
                        );
                        self.workload_manager_handle
                            .unregister_message(self.message_id)
                            .await;
                        return Incomplete;
                    }
                    let priority = self.workload_manager_handle.get_priority(
                        self.message_id,
                        self.expected - self.collected
                    ).await;
                    self.grant(self.collected, priority).await;
                    println!(
                        "MESSAGE RECEIVER (ID: {}) (ID: {}) REQUESTED RESEND",
                        self.datagram.destination_id, self.message_id
                    );
                    resend_counter += 1;
                }
                Some(datagram) = self.rx.recv() => {
                    println!(
                        "MESSAGE RECEIVER (ID: {}) (ID: {}) RECEIVED SCHEDULED DATAGRAM FROM (ID: {})",
                        datagram.destination_id, datagram.message_id, datagram.source_id
                    );
                    self.add_datagram(datagram);
                    if let Complete =  self.check_message_scheduled() {
                        self.workload_manager_handle
                            .unregister_message(self.message_id)
                            .await;
                        return Complete;
                    }
                    let priority = self.workload_manager_handle.get_priority(
                        self.message_id,
                        self.expected - self.collected
                    ).await;
                    self.grant(self.collected, priority).await;
                }
            }
        }
    }

    pub fn check_message_unscheduled(&self) -> UnscheduledState {
        use UnscheduledState::*;
        if self.collected == self.expected {
            return Complete;
        }
        if self.collected >= UNSCHEDULED_HOMA_DATAGRAM_LIMIT as u64 {
            return Complete;
        }
        Incomplete
    }

    pub fn check_message_scheduled(&self) -> ScheduledState {
        use ScheduledState::*;
        if self.collected == self.expected {
            return Complete;
        }
        Incomplete
    }

    async fn grant(&self, sequence_number: u64, priority: u8) {
        let mut grant = HomaDatagram::default();
        grant.message_id = self.message_id;
        grant.datagram_type = HomaDatagramType::Grant;
        grant.source_address = self.destination_address.clone();
        grant.destination_address = self.source_address.clone();
        grant.source_id = self.destination_id;
        grant.destination_id = self.source_id;
        grant.sequence_number = sequence_number;
        grant.priority = priority;
        let grant_ip = grant.to_ipv4(64);
        self.datagram_sender_handle.tx.send(grant_ip).await.unwrap();
    }

    async fn complete(&self) {
        use crate::actors::application::ApplicationMessage::*;
        self.grant(self.expected, 0).await;
        let message = self.build_message();
        self.application_handle
            .tx
            .send(FromMessageReceiver(message))
            .await
            .unwrap();
    }

    pub fn build_message(&self) -> HomaMessage {
        let content = self
            .datagrams
            .iter()
            .map(|datagram_entry| datagram_entry.to_owned().unwrap())
            .map(|datagram| datagram.payload.clone())
            .collect::<Vec<Vec<u8>>>()
            .concat();
        HomaMessageBuilder::default()
            .id(self.message_id)
            .source_address(self.source_address)
            .destination_address(self.destination_address)
            .source_id(self.source_id)
            .destination_id(self.destination_id)
            .content(content)
            .build()
            .unwrap()
    }
}

async fn run_message_receiver(mut message_receiver: MessageReceiver) {
    if let UnscheduledState::Incomplete = message_receiver.receive_unscheduled_datagrams().await {
        println!(
            "MESSAGE RECEIVER (ID: {}) (ID: {}) FAILED TO RECEIVE ALL UNSCHEDULED DATAGRAMS",
            message_receiver.destination_id, message_receiver.message_id,
        );
        return;
    }
    println!(
        "MESSAGE RECEIVER (ID: {}) (ID: {}) RECEIVED ALL UNSCHEDULED DATAGRAMS",
        message_receiver.destination_id, message_receiver.message_id,
    );
    if message_receiver.unscheduled_only {
        println!(
            "MESSAGE RECEIVER (ID: {}) (ID: {}) RECEIVED ALL DATAGRAMS",
            message_receiver.destination_id, message_receiver.message_id,
        );
        message_receiver.complete().await;
        return;
    }
    if let ScheduledState::Incomplete = message_receiver.receive_scheduled_datagrams().await {
        println!(
            "MESSAGE RECEIVER (ID: {}) (ID: {}) FAILED TO RECEIVE ALL SCHEDULED DATAGRAMS",
            message_receiver.destination_id, message_receiver.message_id,
        );
        return;
    }
    println!(
        "MESSAGE RECEIVER (ID: {}) (ID: {}) RECEIVED ALL SCHEDULED DATAGRAMS",
        message_receiver.destination_id, message_receiver.message_id,
    );
    message_receiver.complete().await;
}

#[derive(Clone)]
pub struct MessageReceiverHandle {
    pub tx: Sender<HomaDatagram>,
}

impl MessageReceiverHandle {
    pub fn new(
        datagram: HomaDatagram,
        datagram_sender_handle: DatagramSenderHandle,
        priority_manager_handle: PriorityManagerHandle,
        workload_manager_handle: WorkloadManagerHandle,
        application_handle: ApplicationHandle,
    ) -> (Self, JoinHandle<()>) {
        let (tx, rx) = channel::<HomaDatagram>(1000);

        let expected = (datagram.message_length + (HOMA_DATAGRAM_PAYLOAD_LENGTH as u64) - 1)
            / HOMA_DATAGRAM_PAYLOAD_LENGTH as u64;

        let mut datagrams = vec![None; expected as usize];
        let first_datagram = datagrams
            .get_mut(datagram.sequence_number as usize)
            .unwrap();
        *first_datagram = Some(datagram.clone());
        let unscheduled_only = expected <= UNSCHEDULED_HOMA_DATAGRAM_LIMIT as u64;

        let message_receiver_actor = MessageReceiver {
            datagram_sender_handle,
            application_handle,
            priority_manager_handle,
            workload_manager_handle,
            rx,

            message_id: datagram.message_id,
            datagram: datagram.clone(),
            source_address: datagram.source_address,
            destination_address: datagram.destination_address,
            source_id: datagram.source_id,
            destination_id: datagram.destination_id,
            expected,
            collected: 1,
            unscheduled_only,
            datagrams,
        };
        let join_handle = tokio::spawn(run_message_receiver(message_receiver_actor));
        (Self { tx }, join_handle)
    }
}
