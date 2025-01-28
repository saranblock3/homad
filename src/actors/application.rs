use super::priority_manager::PriorityManagerHandle;
use super::workload_manager::WorkloadManagerHandle;
use super::{
    application_writer::ApplicationWriterHandle, datagram_sender::DatagramSenderHandle,
    message_receiver::MessageReceiverHandle, message_sender::MessageSenderHandle,
};
use crate::actors::application_registrar::ApplicationRegistrarHandle;
use crate::actors::application_registrar::ApplicationRegistrarMessage::FromApplication;
use crate::models::{datagram::HomaDatagram, message::HomaMessage};
use std::collections::HashMap;
use tokio::sync::mpsc::{channel, Receiver, Sender};
use tokio::task::JoinHandle;

#[allow(unused)]
struct Application {
    id: u32,
    rx: Receiver<ApplicationMessage>,
    application_handle: ApplicationHandle,
    application_writer_handle: ApplicationWriterHandle,
    message_receivers: HashMap<u64, (MessageReceiverHandle, JoinHandle<()>)>,
    message_senders: HashMap<u64, (MessageSenderHandle, JoinHandle<()>)>,
    application_registrar_handle: ApplicationRegistrarHandle,
    priority_manager_handle: PriorityManagerHandle,
    workload_manager_handle: WorkloadManagerHandle,
    datagram_sender_handle: DatagramSenderHandle,
}

#[allow(unused)]
impl Application {
    async fn handle_application_message(&mut self, application_message: ApplicationMessage) {
        use ApplicationMessage::*;
        match application_message {
            Shutdown => {
                println!("APPLICATION (ID: {}) SHUTDOWN", self.id);
                self.application_registrar_handle
                    .tx
                    .send(FromApplication(self.id))
                    .await
                    .expect("application -> application_registrar failed");
                self.rx.close()
            }
            FromDatagramReceiver(datagram) => self.handle_from_datagram_receiver(datagram).await,
            FromApplicationReader(message) => self.handle_from_application_reader(message),
            FromMessageReceiver(message) => self.handle_from_message_receiver(message).await,
            FromMessageSender(id) => self.handle_from_message_sender(id),
        }
    }

    async fn handle_shutdown(&mut self) {
        self.rx.close();
    }

    async fn handle_from_datagram_receiver(&mut self, datagram: HomaDatagram) {
        use crate::models::datagram::HomaDatagramType::*;
        println!(
            "APPLICATION (ID: {}) RECEIVED DATAGRAM FROM (ID: {}) WITH MESSAGE ID {}",
            self.id, datagram.source_id, datagram.message_id,
        );
        match datagram.datagram_type {
            Data => self.handle_data_datagram(datagram).await,
            _ => self.handle_control_datagram(datagram).await,
        }
    }

    async fn handle_data_datagram(&mut self, datagram: HomaDatagram) {
        let message_id = datagram.message_id;
        if let Some((message_receiver_handle, _)) = self.message_receivers.get(&datagram.message_id)
        {
            println!(
                "APPLICATION (ID: {}) TO MESSAGE RECEIVER (ID: {}) FROM (ID: {}) WITH SEQUENCE NUMBER {}",
                self.id, datagram.message_id, datagram.source_id, datagram.sequence_number
            );
            message_receiver_handle.tx.send(datagram).await;
            // .expect("application -> message_receiver failed");
        } else {
            println!(
                "APPLICATION (ID: {}) CREATE MESSAGE RECEIVER (ID: {}) FROM (ID: {})",
                self.id, datagram.message_id, datagram.source_id
            );
            let message_receiver_handle = MessageReceiverHandle::new(
                datagram,
                self.datagram_sender_handle.clone(),
                self.priority_manager_handle.clone(),
                self.workload_manager_handle.clone(),
                self.application_handle.clone(),
            );
            self.message_receivers
                .insert(message_id, message_receiver_handle);
        }
    }

    async fn handle_control_datagram(&mut self, datagram: HomaDatagram) {
        if let Some((message_sender_handle, _)) = self.message_senders.get(&datagram.message_id) {
            message_sender_handle.tx.send(datagram).await;
            // .expect("application -> message_sender failed");
        }
    }

    fn handle_from_application_reader(&mut self, message: HomaMessage) {
        let message_id = message.id;
        println!(
            "APPLICATION (ID: {}) CREATE MESSAGE SENDER (ID: {})",
            self.id, message.id
        );
        let message_sender_handle = MessageSenderHandle::new(
            message,
            self.application_handle.clone(),
            self.priority_manager_handle.clone(),
            self.datagram_sender_handle.clone(),
        );
        self.message_senders
            .insert(message_id, message_sender_handle);
    }

    async fn handle_from_message_receiver(&mut self, message: HomaMessage) {
        if let Some((message_receiver_handle, join_handle)) =
            self.message_receivers.remove(&message.id)
        {
            join_handle.abort();
            self.application_writer_handle.tx.send(message).await;
        }
    }

    fn handle_from_message_sender(&mut self, id: u64) {
        if let Some((message_receiver_handle, join_handle)) = self.message_senders.remove(&id) {
            join_handle.abort();
        }
    }
}

#[allow(unused)]
pub enum ApplicationMessage {
    Shutdown,
    FromDatagramReceiver(HomaDatagram),
    FromApplicationReader(HomaMessage),
    FromMessageReceiver(HomaMessage),
    FromMessageSender(u64),
}

async fn run_application(mut application: Application) {
    while let Some(application_message) = application.rx.recv().await {
        application
            .handle_application_message(application_message)
            .await;
    }
}

#[derive(Clone)]
pub struct ApplicationHandle {
    pub tx: Sender<ApplicationMessage>,
}

impl ApplicationHandle {
    pub fn new(
        id: u32,
        application_writer_handle: ApplicationWriterHandle,
        application_registrar_handle: ApplicationRegistrarHandle,
        priority_manager_handle: PriorityManagerHandle,
        workload_manager_handle: WorkloadManagerHandle,
        datagram_sender_handle: DatagramSenderHandle,
    ) -> Self {
        let (tx, rx) = channel::<ApplicationMessage>(1000);
        let application_handle = Self { tx };
        let application = Application {
            id,
            rx,
            application_handle: application_handle.clone(),
            application_writer_handle,
            message_receivers: HashMap::new(),
            message_senders: HashMap::new(),
            application_registrar_handle,
            priority_manager_handle,
            workload_manager_handle,
            datagram_sender_handle,
        };
        tokio::spawn(run_application(application));
        application_handle
    }
}
