use super::priority_manager::PriorityManagerHandle;
use super::workload_manager::WorkloadManagerHandle;
use super::{
    application_writer::ApplicationWriterHandle, datagram_sender::DatagramSenderHandle,
    message_receiver::MessageReceiverHandle, message_sender::MessageSenderHandle,
};
use crate::actors::application_registrar::ApplicationRegistrarHandle;
use crate::actors::application_registrar::ApplicationRegistrarMessage::FromApplication;
use crate::models::{datagram::HomaDatagram, message::HomaMessage};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::{Duration, SystemTime};
use tokio::select;
use tokio::sync::mpsc::error::SendError;
use tokio::sync::mpsc::{channel, Receiver, Sender};
use tokio::sync::Mutex;
use tokio::task::JoinHandle;
use tokio::time::sleep;

#[allow(unused)]
struct Application {
    id: u32,
    rx: Receiver<ApplicationMessage>,
    application_handle: ApplicationHandle,
    application_writer_handle: ApplicationWriterHandle,
    message_receivers: Arc<Mutex<HashMap<u64, (MessageReceiverHandle, JoinHandle<()>)>>>,
    message_senders: Arc<Mutex<HashMap<u64, (MessageSenderHandle, JoinHandle<()>)>>>,
    delivered_messages: HashSet<u64>,
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
            Shutdown => self.handle_shutdown().await,
            FromDatagramReceiver(datagram) => self.handle_from_datagram_receiver(datagram).await,
            FromApplicationReader(message) => self.handle_from_application_reader(message).await,
            FromMessageReceiver(message) => self.handle_from_message_receiver(message).await,
            FromMessageSender(id) => self.handle_from_message_sender(id).await,
        }
    }

    async fn handle_shutdown(&mut self) {
        self.application_registrar_handle
            .send(FromApplication(self.id))
            .await;
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
        if self.delivered_messages.contains(&message_id) {
            return;
        }
        let mut message_receivers = self.message_receivers.lock().await;
        if let Some((message_receiver_handle, _)) = message_receivers.get(&message_id) {
            message_receiver_handle.tx.send(datagram).await;
            return;
        }
        let message_receiver_handle = MessageReceiverHandle::new(
            datagram,
            self.datagram_sender_handle.clone(),
            self.priority_manager_handle.clone(),
            self.workload_manager_handle.clone(),
            self.application_handle.clone(),
        );
        message_receivers.insert(message_id, message_receiver_handle);
    }

    async fn handle_control_datagram(&mut self, datagram: HomaDatagram) {
        if let Some((message_sender_handle, _)) =
            self.message_senders.lock().await.get(&datagram.message_id)
        {
            message_sender_handle.tx.send(datagram).await;
        }
    }

    async fn handle_from_application_reader(&mut self, message: HomaMessage) {
        let message_id = message.id;
        let mut message_senders = self.message_senders.lock().await;
        if message_senders.contains_key(&message_id) {
            return;
        }
        let now = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap();
        println!(
            "(ID: {}) SENT AT TIME - {} WITH SIZE {}",
            message.id,
            now.as_millis(),
            message.content.len()
        );
        let message_sender_handle = MessageSenderHandle::new(
            message,
            self.application_handle.clone(),
            self.priority_manager_handle.clone(),
            self.workload_manager_handle.clone(),
            self.datagram_sender_handle.clone(),
        );
        message_senders.insert(message_id, message_sender_handle);
        let now = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap();
    }

    async fn handle_from_message_receiver(&mut self, message: HomaMessage) {
        self.delivered_messages.insert(message.id);
        let now = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap();
        println!(
            "(ID: {}) DELIVERED AT TIME - {} WITH SIZE {}",
            message.id,
            now.as_millis(),
            message.content.len()
        );
        if let Some((message_receiver_handle, join_handle)) =
            self.message_receivers.lock().await.remove(&message.id)
        {
            self.application_writer_handle.tx.send(message).await;
            join_handle.abort();
        }
    }

    async fn handle_from_message_sender(&mut self, id: u64) {
        if let Some((message_receiver_handle, join_handle)) =
            self.message_senders.lock().await.remove(&id)
        {
            join_handle.abort();
        }
    }
}

#[allow(unused)]
#[derive(Debug)]
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
    tx: Sender<ApplicationMessage>,
    pub message_senders: Arc<Mutex<HashMap<u64, (MessageSenderHandle, JoinHandle<()>)>>>,
    pub message_receivers: Arc<Mutex<HashMap<u64, (MessageReceiverHandle, JoinHandle<()>)>>>,
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
        let (tx, rx) = channel::<ApplicationMessage>(100000);
        let message_senders = Arc::new(Mutex::new(HashMap::new()));
        let message_receivers = Arc::new(Mutex::new(HashMap::new()));
        let application_handle = Self {
            tx,
            message_senders: Arc::clone(&message_senders),
            message_receivers: Arc::clone(&message_receivers),
        };
        let application = Application {
            id,
            rx,
            application_handle: application_handle.clone(),
            application_writer_handle,
            message_receivers,
            message_senders,
            delivered_messages: HashSet::new(),
            application_registrar_handle,
            priority_manager_handle,
            workload_manager_handle,
            datagram_sender_handle,
        };
        tokio::spawn(run_application(application));
        application_handle
    }

    pub async fn send(
        &self,
        application_message: ApplicationMessage,
    ) -> Result<(), SendError<ApplicationMessage>> {
        self.tx.send(application_message).await
    }

    pub fn blocking_send(
        &self,
        application_message: ApplicationMessage,
    ) -> Result<(), SendError<ApplicationMessage>> {
        self.tx.blocking_send(application_message)
    }
}
