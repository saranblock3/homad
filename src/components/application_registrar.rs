/*
ApplicationRegistrar actor

This actor is responsible for registering/creating new Applications,
checking that there is no existing application with the same id

It also listens for Applications shutting down and degestering them
*/
use crate::components::application::ApplicationHandle;
use crate::components::datagram_sender::DatagramSenderHandle;
use crate::components::priority_manager::PriorityManagerHandle;
use crate::components::workload_manager::WorkloadManagerHandle;
use crate::models::registration::HomaRegistrationMessage;
use std::collections::HashMap;
use std::os::unix::net::UnixStream;
use std::sync::Arc;
use std::sync::Mutex;
use tokio::sync::mpsc::channel;
use tokio::sync::mpsc::error::SendError;
use tokio::sync::mpsc::Receiver;
use tokio::sync::mpsc::Sender;
use tokio::task::JoinHandle;

pub struct ApplicationRegistrar {
    rx: Receiver<ApplicationRegistrarMessage>,

    application_handles: Arc<Mutex<HashMap<u32, ApplicationHandle>>>,
    application_join_handles: HashMap<u32, JoinHandle<()>>,
    application_registrar_handle: ApplicationRegistrarHandle,
    datagram_sender_handle: DatagramSenderHandle,
    priority_manager_handle: PriorityManagerHandle,
    workload_manager_handle: WorkloadManagerHandle,
}

impl ApplicationRegistrar {
    // Get registration message from new stream and create the application
    fn handle_from_application_listener(&mut self, mut stream: UnixStream) -> Result<(), String> {
        match HomaRegistrationMessage::from_unix_stream(&mut stream) {
            Ok(registration_message) => self.create_application(registration_message, stream),
            Err(e) => {
                println!("registration {}", e);
                Err(e)
            }
        }
    }

    // Check Application with the id does not exist and then
    // spawn the Application
    fn create_application(
        &mut self,
        registration_message: HomaRegistrationMessage,
        stream: UnixStream,
    ) -> Result<(), String> {
        let mut application_handles = self.application_handles.lock().unwrap();
        let id = registration_message.application_id;

        match application_handles.get(&id) {
            Some(_) => Err("ApplicationRegistrar tried to create existing application".to_string()),
            _ => {
                let (application_handle, join_handle) = ApplicationHandle::new(
                    id,
                    stream,
                    self.application_registrar_handle.clone(),
                    self.datagram_sender_handle.clone(),
                    self.priority_manager_handle.clone(),
                    self.workload_manager_handle.clone(),
                )?;
                application_handles.insert(id, application_handle.clone());
                self.application_join_handles.insert(id, join_handle);
                Ok(())
            }
        }
    }

    // Listen for Application shutting down and remove from the register
    fn handle_from_application(&mut self, id: u32) {
        let mut application_handles = self.application_handles.lock().unwrap();
        application_handles.remove(&id);
        if let Some(join_handle) = self.application_join_handles.get(&id) {
            join_handle.abort();
        }
    }
}

pub enum ApplicationRegistrarMessage {
    FromApplicationListener(UnixStream),
    FromApplication(u32),
}

// Multiplex ApplicationRegistrarMessages and handle them
fn run_application_registrar(mut application_registrar: ApplicationRegistrar) {
    use ApplicationRegistrarMessage::*;
    while let Some(application_registrar_message) = application_registrar.rx.blocking_recv() {
        match application_registrar_message {
            FromApplicationListener(stream) => {
                let _ = application_registrar.handle_from_application_listener(stream);
            }
            FromApplication(id) => application_registrar.handle_from_application(id),
        }
    }
}

#[derive(Clone)]
pub struct ApplicationRegistrarHandle {
    tx: Sender<ApplicationRegistrarMessage>,
}

impl ApplicationRegistrarHandle {
    // Initialize ApplicationRegister and return the handle
    pub fn new(
        application_handles: Arc<Mutex<HashMap<u32, ApplicationHandle>>>,
        priority_manager_handle: PriorityManagerHandle,
        workload_manager_handle: WorkloadManagerHandle,
        datagram_sender_handle: DatagramSenderHandle,
    ) -> Self {
        let (tx, rx) = channel::<ApplicationRegistrarMessage>(1000);
        let application_registrar_handle = Self { tx };
        let application_registrar = ApplicationRegistrar {
            rx,
            application_handles,
            application_join_handles: HashMap::new(),
            application_registrar_handle: application_registrar_handle.clone(),
            priority_manager_handle,
            workload_manager_handle,
            datagram_sender_handle,
        };
        tokio::task::spawn_blocking(move || run_application_registrar(application_registrar));
        application_registrar_handle
    }

    // Async send to ApplicationRegistrar through ApplicationRegistrarHandle
    pub async fn send(
        &self,
        application_registrar_message: ApplicationRegistrarMessage,
    ) -> Result<(), SendError<ApplicationRegistrarMessage>> {
        self.tx.send(application_registrar_message).await
    }

    // Blocking send to ApplicationRegistrar through ApplicationRegistrarHandle
    pub fn blocking_send(
        &self,
        application_registrar_message: ApplicationRegistrarMessage,
    ) -> Result<(), SendError<ApplicationRegistrarMessage>> {
        self.tx.blocking_send(application_registrar_message)
    }
}
