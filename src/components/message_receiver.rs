/*
MessageReceiver actor

This actor is responsible for receiving all the datagrams of a message

The actor first receives all unscheduled datagrams and issues resends if
they do not arrive within a specific timeout, if all unscheduled datagrams are
not received after a maximum number of resend requests, the MessageReceiver exits

The actor then receives all scheduled datagram. The actor must send a grant for each
scheduled datagram. If the subsequent data datagram is not received within a timeout,
the actor sends a duplicate grant. If the data datagaram is not received after a maximum
number of duplicate grants, the MessageReceiver exits
*/
use crate::components::application::ApplicationHandle;
use crate::components::application_writer::ApplicationWriterHandle;
use crate::components::datagram_sender::DatagramSenderHandle;
use crate::components::priority_manager::PriorityManagerHandle;
use crate::components::workload_manager::WorkloadManagerHandle;
use crate::config::CONFIG;
use crate::config::CONST;
use crate::models::datagram::HomaDatagram;
use crate::models::datagram::HomaDatagramType;
use crate::models::message::HomaMessage;
use crate::models::message::HomaMessageBuilder;
use crate::utils::fuzz_timeout;
use std::net::Ipv4Addr;
use tokio::select;
use tokio::sync::mpsc::channel;
use tokio::sync::mpsc::Receiver;
use tokio::sync::mpsc::Sender;
use tokio::task::JoinHandle;
use tokio::time::sleep;
use tokio::time::Duration;

enum UnscheduledState {
    Incomplete,
    Complete,
}

enum ScheduledState {
    Incomplete,
    Complete,
}

struct MessageReceiver {
    message_id: u64,
    rx: Receiver<HomaDatagram>,

    source_address: Ipv4Addr,
    destination_address: Ipv4Addr,

    source_id: u32,
    destination_id: u32,
    priority: u8,
    local_workload: [u64; CONST::UNSCHEDULED_PRIORITY_PARTITIONS],
    remote_workload: [u64; CONST::UNSCHEDULED_PRIORITY_PARTITIONS],
    message_length: u64,
    datagrams: Vec<Option<HomaDatagram>>,

    expected_datagrams: u32,
    collected_datagrams: u32,
    collected_bytes: u64,
    unscheduled_only: bool,

    application_handle: ApplicationHandle,
    application_writer_handle: ApplicationWriterHandle,
    datagram_sender_handle: DatagramSenderHandle,
    priority_manager_handle: PriorityManagerHandle,
    workload_manager_handle: WorkloadManagerHandle,
}

impl MessageReceiver {
    // Add the received datagram to the vector of datagrams if it has not yet been received
    fn add_datagram(&mut self, datagram: HomaDatagram) -> u64 {
        if let Some(datagram_entry) = self.datagrams.get_mut(datagram.sequence_number as usize) {
            if datagram_entry.is_none() {
                self.collected_datagrams += 1;
                self.collected_bytes += datagram.payload.len() as u64;
                *datagram_entry = Some(datagram);
            }
        }
        self.datagrams.len() as u64
    }

    // Send resend requests for all unscheduled datagrams which have not yet been received
    async fn request_resend_unscheduled_datagrams(&mut self) {
        let mut resend: HomaDatagram = HomaDatagram::default();
        resend.message_id = self.message_id;
        resend.datagram_type = HomaDatagramType::Resend;
        resend.source_id = self.destination_id;
        resend.destination_id = self.source_id;
        resend.workload = self.local_workload.clone();
        for i in 0..CONFIG.UNSCHEDULED_DATAGRAM_LIMIT {
            if let Some(None) = self.datagrams.get(i) {
                let mut resend = resend.clone();
                resend.sequence_number = i as u32;
                let _ = resend.checksum();
                let packet = resend.to_ipv4(
                    self.destination_address.clone(),
                    self.source_address.clone(),
                    56,
                );
                self.datagram_sender_handle
                    .send(packet)
                    .await
                    .expect("MessageReceiver -> DatagramSender failed");
            }
        }
    }

    async fn receive_unscheduled_datagrams(&mut self) -> UnscheduledState {
        use UnscheduledState::*;
        let mut resend_counter = 0;
        loop {
            if let Complete = self.check_message_unscheduled() {
                return Complete;
            }
            let timeout = fuzz_timeout(CONFIG.TIMEOUT);
            select! {
                _ = sleep(Duration::from_millis(timeout))=> {
                    if resend_counter == CONFIG.RESENDS {
                        return Incomplete
                    }
                    self.request_resend_unscheduled_datagrams().await;
                    resend_counter += 1
                }
                Some(datagram) = self.rx.recv() => {
                    self.add_datagram(datagram);
                }
            }
        }
    }

    async fn receive_scheduled_datagrams(&mut self) -> ScheduledState {
        use ScheduledState::*;
        self.register_priority().await;
        self.get_priority().await;
        self.grant(self.collected_datagrams).await;
        let mut resend_counter = 0;
        loop {
            let timeout = fuzz_timeout(CONFIG.TIMEOUT);
            select! {
                _ = sleep(Duration::from_millis(timeout)) => {
                    if resend_counter == CONFIG.LARGE_RESENDS {
                        self.unregister_priority().await;
                        return Incomplete;
                    }
                    self.get_priority().await;
                    self.grant(self.collected_datagrams).await;
                    resend_counter += 1;
                }
                Some(datagram) = self.rx.recv() => {
                    self.add_datagram(datagram);
                    if let Complete =  self.check_message_scheduled() {
                        self.unregister_priority().await;
                        return Complete;
                    }
                    self.get_priority().await;
                    self.grant(self.collected_datagrams).await;
                }
            }
        }
    }

    async fn get_priority(&mut self) {
        self.priority = self
            .priority_manager_handle
            .get_scheduled_priority(self.message_id, self.message_length - self.collected_bytes)
            .await;
    }

    async fn register_priority(&self) {
        self.priority_manager_handle
            .register_scheduled_message(self.message_id, self.message_length - self.collected_bytes)
            .await;
    }

    async fn unregister_priority(&self) {
        self.priority_manager_handle
            .unregister_scheduled_message(self.message_id)
            .await;
    }

    async fn put_remote_workload(&self) {
        self.priority_manager_handle
            .put_unscheduled_priority_level_partitions(self.source_address, self.remote_workload)
            .await;
    }

    async fn get_local_workload(&mut self) {
        self.local_workload = self
            .workload_manager_handle
            .update_workload(self.message_length)
            .await
            .unwrap();
    }

    fn check_message_unscheduled(&self) -> UnscheduledState {
        use UnscheduledState::*;
        if self.collected_bytes == self.message_length {
            return Complete;
        }
        if self.collected_datagrams >= CONFIG.UNSCHEDULED_DATAGRAM_LIMIT as u32 {
            return Complete;
        }
        Incomplete
    }

    fn check_message_scheduled(&self) -> ScheduledState {
        use ScheduledState::*;
        if self.collected_bytes == self.message_length {
            return Complete;
        }
        Incomplete
    }

    async fn grant(&mut self, sequence_number: u32) {
        let mut grant = HomaDatagram::default();
        grant.message_id = self.message_id;
        grant.datagram_type = HomaDatagramType::Grant;
        grant.source_id = self.destination_id;
        grant.destination_id = self.source_id;
        grant.sequence_number = sequence_number;
        grant.priority = self.priority;
        grant.workload = self.local_workload.clone();
        let _ = grant.checksum();
        let grant_ip = grant.to_ipv4(
            self.destination_address.clone(),
            self.source_address.clone(),
            56,
        );
        self.datagram_sender_handle
            .send(grant_ip)
            .await
            .expect("MessageReceiver -> DatagramSender failed");
    }

    async fn complete(&mut self) {
        use crate::components::application::ApplicationMessage::*;
        let message = self.build_message();
        let message_id = message.id;
        self.rx.close();
        let _ = self.application_writer_handle.tx.send(message).await;
        let _ = self
            .application_handle
            .send(FromMessageReceiver(message_id))
            .await;
        self.grant(self.expected_datagrams).await;
    }

    async fn exit(&mut self) {
        use crate::components::application::ApplicationMessage::*;
        let _ = self
            .application_handle
            .send(FromMessageReceiver(self.message_id))
            .await;
    }

    fn build_message(&self) -> HomaMessage {
        let content = self
            .datagrams
            .iter()
            .map(|datagram_entry| datagram_entry.to_owned().unwrap())
            .map(|datagram| datagram.payload.clone())
            .collect::<Vec<Vec<u8>>>()
            .concat();
        HomaMessageBuilder::default()
            .id(self.message_id)
            .source_address(self.source_address.octets())
            .destination_address(self.destination_address.octets())
            .source_id(self.source_id)
            .destination_id(self.destination_id)
            .content(content)
            .build()
            .unwrap()
    }
}

async fn run_message_receiver(mut message_receiver: MessageReceiver) {
    message_receiver.put_remote_workload().await;
    message_receiver.get_local_workload().await;

    if let UnscheduledState::Incomplete = message_receiver.receive_unscheduled_datagrams().await {
        message_receiver.exit().await;
        return;
    }
    if message_receiver.unscheduled_only {
        message_receiver.complete().await;
        return;
    }
    if let ScheduledState::Incomplete = message_receiver.receive_scheduled_datagrams().await {
        message_receiver.exit().await;
        return;
    }
    message_receiver.complete().await;
}

#[derive(Clone)]
pub struct MessageReceiverHandle {
    pub tx: Sender<HomaDatagram>,
}

impl MessageReceiverHandle {
    pub fn new(
        datagram: HomaDatagram,
        source_address: Ipv4Addr,
        destination_address: Ipv4Addr,

        application_handle: ApplicationHandle,
        application_writer_handle: ApplicationWriterHandle,
        datagram_sender_handle: DatagramSenderHandle,
        priority_manager_handle: PriorityManagerHandle,
        workload_manager_handle: WorkloadManagerHandle,
    ) -> (Self, JoinHandle<()>) {
        let (tx, rx) = channel::<HomaDatagram>(1000);

        let message_length = datagram.message_length;
        let expected_datagrams = ((message_length + CONFIG.DATAGRAM_PAYLOAD_LENGTH as u64 - 1)
            / CONFIG.DATAGRAM_PAYLOAD_LENGTH as u64) as u32;

        let mut datagrams = vec![None; expected_datagrams as usize];
        let first_datagram = datagrams
            .get_mut(datagram.sequence_number as usize)
            .unwrap();
        *first_datagram = Some(datagram.clone());
        let unscheduled_only = message_length
            <= CONFIG.UNSCHEDULED_DATAGRAM_LIMIT as u64 * CONFIG.DATAGRAM_PAYLOAD_LENGTH as u64;

        let message_receiver_actor = MessageReceiver {
            message_id: datagram.message_id,
            rx,

            source_address,
            destination_address,

            source_id: datagram.source_id,
            destination_id: datagram.destination_id,
            local_workload: [0; CONST::UNSCHEDULED_PRIORITY_PARTITIONS],
            priority: 0,
            remote_workload: datagram.workload,
            message_length,
            datagrams,

            expected_datagrams,
            collected_bytes: datagram.payload.len() as u64,
            collected_datagrams: 1,
            unscheduled_only,

            application_handle,
            application_writer_handle,
            datagram_sender_handle,
            priority_manager_handle,
            workload_manager_handle,
        };
        let join_handle = tokio::spawn(run_message_receiver(message_receiver_actor));
        (Self { tx }, join_handle)
    }
}
