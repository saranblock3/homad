/*
Application actor

This actor listens to the corresponding ApplicationReader and
spawns a MessageSender when it is notified of a HomaMessage to be sent.

It also listens to the DatagramReceiver for incoming HomaDatagrams, spawns
a MessageReceiver if one does not exist or forwards the HomaDatagram to the
correct MessageReceiver if one exists.

Finally, it listens to all relevant MessageReceivers for (in)completion
*/
use crate::components::application_reader::ApplicationReader;
use crate::components::application_registrar::ApplicationRegistrarHandle;
use crate::components::application_registrar::ApplicationRegistrarMessage::FromApplication;
use crate::components::application_writer::ApplicationWriterHandle;
use crate::components::datagram_sender::DatagramSenderHandle;
use crate::components::message_receiver::MessageReceiverHandle;
use crate::components::message_sender::MessageSenderHandle;
use crate::components::priority_manager::PriorityManagerHandle;
use crate::components::workload_manager::WorkloadManagerHandle;
use crate::models::datagram::HomaDatagram;
use crate::models::message::HomaMessage;
use crate::utils::split_unix_stream;
use std::collections::HashMap;
use std::collections::HashSet;
use std::net::Ipv4Addr;
use std::os::unix::net::UnixStream;
use std::sync::Arc;
use tokio::sync::mpsc::channel;
use tokio::sync::mpsc::error::SendError;
use tokio::sync::mpsc::Receiver;
use tokio::sync::mpsc::Sender;
use tokio::sync::Mutex;
use tokio::task::JoinHandle;

#[allow(unused)]
struct Application {
    application_id: u32,
    rx: Receiver<ApplicationMessage>,

    // Set to keep track of all messages that have been delivered
    // in order to discard delayed or duplicated datagrams
    delivered_messages: HashSet<u64>,

    // Join handles to abort spawned futures when the
    // application shuts down, or when the futures complete
    application_reader_join_handle: JoinHandle<()>,
    application_writer_join_handle: JoinHandle<()>,
    message_receiver_join_handles: HashMap<u64, JoinHandle<()>>,
    message_sender_join_handles: HashMap<u64, JoinHandle<()>>,

    // Actor handles to contact other relevant actors,
    // the application_handle is a clone of the handle
    // corresponding to this Application actor
    application_handle: ApplicationHandle,
    application_registrar_handle: ApplicationRegistrarHandle,
    application_writer_handle: ApplicationWriterHandle,
    datagram_sender_handle: DatagramSenderHandle,
    message_receiver_handles: Arc<Mutex<HashMap<u64, MessageReceiverHandle>>>,
    message_sender_handles: Arc<Mutex<HashMap<u64, MessageSenderHandle>>>,
    priority_manager_handle: PriorityManagerHandle,
    workload_manager_handle: WorkloadManagerHandle,
}

#[allow(unused)]
impl Application {
    // Multiplex and handle ApplicationMessage types
    async fn handle_application_message(&mut self, application_message: ApplicationMessage) {
        use ApplicationMessage::*;
        match application_message {
            Shutdown => self.handle_shutdown().await,
            FromDatagramReceiver(datagram, source_address, destination_address) => {
                self.handle_from_datagram_receiver(datagram, source_address, destination_address)
                    .await
            }
            FromApplicationReader(message) => self.handle_from_application_reader(message).await,
            FromMessageReceiver(message_id) => self.handle_from_message_receiver(message_id).await,
            FromMessageSender(id) => self.handle_from_message_sender(id).await,
        }
    }

    // Upon shutdown close the receiving channel,
    // disconnect and abort all connected actors,
    // inform the ApplicationRegistrar
    async fn handle_shutdown(&mut self) {
        self.rx.close();

        let mut message_receiver_handles = self.message_receiver_handles.lock().await;
        message_receiver_handles.clear();
        let mut message_sender_handles = self.message_sender_handles.lock().await;
        message_sender_handles.clear();

        self.message_receiver_join_handles
            .iter()
            .map(|(id, join_handle)| join_handle.abort());
        self.message_sender_join_handles
            .iter()
            .map(|(id, join_handle)| join_handle.abort());

        self.application_writer_join_handle.abort();
        self.application_reader_join_handle.abort();

        self.application_registrar_handle
            .send(FromApplication(self.application_id))
            .await;
    }

    // Multiplex and handle data and control datagrams
    async fn handle_from_datagram_receiver(
        &mut self,
        datagram: HomaDatagram,
        source_address: Ipv4Addr,
        destination_address: Ipv4Addr,
    ) {
        use crate::models::datagram::HomaDatagramType::*;
        match datagram.datagram_type {
            Data => {
                self.handle_data_datagram(datagram, source_address, destination_address)
                    .await
            }
            _ => self.handle_control_datagram(datagram).await,
        }
    }

    // Foward datagram to existing MessageSender
    // or spawn a new one
    async fn handle_data_datagram(
        &mut self,
        datagram: HomaDatagram,
        source_address: Ipv4Addr,
        destination_address: Ipv4Addr,
    ) {
        let message_id = datagram.message_id;
        if self.delivered_messages.contains(&message_id) {
            return;
        }

        let mut message_receivers = self.message_receiver_handles.lock().await;
        if let Some(message_receiver_handle) = message_receivers.get(&message_id) {
            message_receiver_handle.tx.send(datagram).await;
            return;
        }

        let (message_receiver_handle, join_handle) = MessageReceiverHandle::new(
            datagram,
            source_address,
            destination_address,
            self.application_handle.clone(),
            self.application_writer_handle.clone(),
            self.datagram_sender_handle.clone(),
            self.priority_manager_handle.clone(),
            self.workload_manager_handle.clone(),
        );
        message_receivers.insert(message_id, message_receiver_handle);
        self.message_receiver_join_handles
            .insert(message_id, join_handle);
    }

    // Forward datagram to MessageSender
    async fn handle_control_datagram(&mut self, datagram: HomaDatagram) {
        if let Some(message_sender_handle) = self
            .message_sender_handles
            .lock()
            .await
            .get(&datagram.message_id)
        {
            message_sender_handle.tx.send(datagram).await;
        }
    }

    // Spawn a new MessageSender after receiving message from ApplicationReader
    async fn handle_from_application_reader(&mut self, mut message: HomaMessage) {
        message.id = rand::random();
        let message_id = message.id;
        let mut message_senders = self.message_sender_handles.lock().await;
        if message_senders.contains_key(&message_id) {
            return;
        }
        let (message_sender_handle, join_handle) = MessageSenderHandle::new(
            message,
            self.application_handle.clone(),
            self.datagram_sender_handle.clone(),
            self.priority_manager_handle.clone(),
            self.workload_manager_handle.clone(),
        );
        message_senders.insert(message_id, message_sender_handle);
        self.message_sender_join_handles
            .insert(message_id, join_handle);
    }

    // Disconnect and abort MessageReceiver
    async fn handle_from_message_receiver(&mut self, id: u64) {
        self.delivered_messages.insert(id);
        self.message_receiver_handles.lock().await.remove(&id);
        if let Some(join_handle) = self.message_receiver_join_handles.remove(&id) {
            join_handle.abort();
        }
    }

    // Disconnect and abort MessageSender
    async fn handle_from_message_sender(&mut self, id: u64) {
        self.message_sender_handles.lock().await.remove(&id);
        if let Some(join_handle) = self.message_sender_join_handles.remove(&id) {
            join_handle.abort();
        }
    }
}

#[allow(unused)]
#[derive(Debug)]
pub enum ApplicationMessage {
    Shutdown,
    FromDatagramReceiver(HomaDatagram, Ipv4Addr, Ipv4Addr),
    FromApplicationReader(HomaMessage),
    FromMessageReceiver(u64),
    FromMessageSender(u64),
}

// Receive ApplicationMessages and handle them
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
    pub message_senders: Arc<Mutex<HashMap<u64, MessageSenderHandle>>>,
    pub message_receivers: Arc<Mutex<HashMap<u64, MessageReceiverHandle>>>,
}

impl ApplicationHandle {
    // Start Application actor, return the actor and join jandles
    pub fn new(
        id: u32,
        stream: UnixStream,

        application_registrar_handle: ApplicationRegistrarHandle,
        datagram_sender_handle: DatagramSenderHandle,
        priority_manager_handle: PriorityManagerHandle,
        workload_manager_handle: WorkloadManagerHandle,
    ) -> Result<(Self, JoinHandle<()>), String> {
        let (read_stream, write_stream) = split_unix_stream(stream)?;

        let (application_writer_handle, application_writer_join_handle) =
            ApplicationWriterHandle::new(write_stream);

        let (tx, rx) = channel::<ApplicationMessage>(1000);

        let message_senders = Arc::new(Mutex::new(HashMap::new()));
        let message_receivers = Arc::new(Mutex::new(HashMap::new()));

        let application_handle = Self {
            tx,
            message_senders: Arc::clone(&message_senders),
            message_receivers: Arc::clone(&message_receivers),
        };

        let application_reader_join_handle =
            ApplicationReader::start(read_stream, application_handle.clone());

        let application = Application {
            application_id: id,
            rx,

            delivered_messages: HashSet::new(),

            application_reader_join_handle,
            application_writer_join_handle,
            message_receiver_join_handles: HashMap::new(),
            message_sender_join_handles: HashMap::new(),

            application_handle: application_handle.clone(),
            application_registrar_handle,
            application_writer_handle,
            datagram_sender_handle,
            message_receiver_handles: message_receivers,
            message_sender_handles: message_senders,
            priority_manager_handle,
            workload_manager_handle,
        };
        let join_handle = tokio::spawn(run_application(application));

        Ok((application_handle, join_handle))
    }

    // Async send an ApplicationMessage to the Application actor
    pub async fn send(
        &self,
        application_message: ApplicationMessage,
    ) -> Result<(), SendError<ApplicationMessage>> {
        self.tx.send(application_message).await
    }

    // Blocking send an ApplicationMessage to the Application actor
    pub fn blocking_send(
        &self,
        application_message: ApplicationMessage,
    ) -> Result<(), SendError<ApplicationMessage>> {
        self.tx.blocking_send(application_message)
    }

    // Send data HomaDatagram to existing MessageReceiver
    // or send to Application
    fn handle_data_datagram(
        &self,
        datagram: HomaDatagram,
        source_address: Ipv4Addr,
        destination_address: Ipv4Addr,
    ) {
        use super::application::ApplicationMessage::FromDatagramReceiver;
        if let Some(message_receiver_handle) = self
            .message_receivers
            .blocking_lock()
            .get(&datagram.message_id)
        {
            let _ = message_receiver_handle.tx.blocking_send(datagram);
            return;
        }
        let _ = self.blocking_send(FromDatagramReceiver(
            datagram,
            source_address,
            destination_address,
        ));
    }

    // Send control HomaDatagram to existing MessageSender
    fn handle_control_datagram(&self, datagram: HomaDatagram) {
        if let Some(message_sender_handle) = self
            .message_senders
            .blocking_lock()
            .get(&datagram.message_id)
        {
            let _ = message_sender_handle.tx.blocking_send(datagram);
        }
    }

    // Multiplex and handle HomaDatagram types
    pub fn blocking_send_datagram(
        &self,
        datagram: HomaDatagram,
        source_address: Ipv4Addr,
        destination_address: Ipv4Addr,
    ) {
        use crate::models::datagram::HomaDatagramType::*;
        match datagram.datagram_type {
            Data => {
                self.handle_data_datagram(datagram, source_address, destination_address);
            }
            _ => {
                self.handle_control_datagram(datagram);
            }
        }
    }
}
