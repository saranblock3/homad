use super::{
    application_writer::ApplicationWriterHandle, datagram_sender::DatagramSenderHandle,
    message_receiver::MessageReceiverHandle, message_sender::MessageSenderHandle,
};
use crate::models::{datagram::HomaDatagram, message::HomaMessage};
use std::collections::HashMap;
use tokio::sync::mpsc::{channel, Receiver, Sender};

#[allow(unused)]
struct Application {
    id: u32,
    rx: Receiver<ApplicationMessage>,
    application_handle: ApplicationHandle,
    application_writer_handle: ApplicationWriterHandle,
    message_receivers: HashMap<u64, MessageReceiverHandle>,
    message_senders: HashMap<u64, MessageSenderHandle>,
    datagram_sender_handle: DatagramSenderHandle,
}

#[allow(unused)]
impl Application {
    async fn handle_application_message(&mut self, application_message: ApplicationMessage) {
        use ApplicationMessage::*;
        match application_message {
            Shutdown => self.rx.close(),
            FromDatagramReceiver(datagram) => self.handle_from_datagram_receiver(datagram).await,
            FromApplicationReader(message) => self.handle_from_application_reader(message),
            ToApplicationWriter(message) => self.handle_to_application_writer(message).await,
        }
    }

    async fn handle_shutdown(&mut self) {
        self.rx.close();
    }

    async fn handle_from_datagram_receiver(&mut self, datagram: HomaDatagram) {
        use crate::models::datagram::HomaDatagramType::*;
        match datagram.datagram_type {
            Data => self.handle_data_datagram(datagram).await,
            _ => self.handle_control_datagram(datagram).await,
        }
    }

    async fn handle_data_datagram(&mut self, datagram: HomaDatagram) {
        let message_id = datagram.message_id;
        if let Some(message_receiver_handle) = self.message_receivers.get(&datagram.message_id) {
            println!(
                "{} -> {}\n{:?}",
                datagram.source_id, datagram.destination_id, datagram
            );
            message_receiver_handle.tx.send(datagram).await.unwrap();
            println!("Sent to message receiver {}", message_id);
        } else {
            let message_receiver_handle = MessageReceiverHandle::new(
                datagram,
                self.datagram_sender_handle.clone(),
                self.application_handle.clone(),
            );
            self.message_receivers
                .insert(message_id, message_receiver_handle);
        }
    }

    async fn handle_control_datagram(&mut self, datagram: HomaDatagram) {
        if let Some(message_receiver_handle) = self.message_senders.get(&datagram.message_id) {
            println!(
                "{} -> {}\n{:?}",
                datagram.source_id, datagram.destination_id, datagram
            );
            message_receiver_handle.tx.send(datagram).await.unwrap();
        }
    }

    fn handle_from_application_reader(&mut self, message: HomaMessage) {
        let message_id = message.id;
        let message_sender_handle =
            MessageSenderHandle::new(message, self.datagram_sender_handle.clone());
        self.message_senders
            .insert(message_id, message_sender_handle);
    }

    async fn handle_to_application_writer(&mut self, message: HomaMessage) {
        self.application_writer_handle.tx.send(message).await;
    }
}

#[allow(unused)]
pub enum ApplicationMessage {
    Shutdown,
    FromDatagramReceiver(HomaDatagram),
    FromApplicationReader(HomaMessage),
    ToApplicationWriter(HomaMessage),
}

async fn run_application(mut application: Application) {
    println!("Started application");
    while let Some(application_message) = application.rx.recv().await {
        println!("Received appliccation message");
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
        datagram_sender_handle: DatagramSenderHandle,
    ) -> Self {
        let (tx, rx) = channel::<ApplicationMessage>(100);
        let application_handle = Self { tx };
        let application = Application {
            id,
            rx,
            application_handle: application_handle.clone(),
            application_writer_handle,
            message_receivers: HashMap::new(),
            message_senders: HashMap::new(),
            datagram_sender_handle,
        };
        tokio::spawn(run_application(application));
        application_handle
    }
}
