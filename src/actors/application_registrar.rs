use super::priority_manager::PriorityManagerHandle;
use super::{
    application::ApplicationHandle, application_reader::ApplicationReader,
    datagram_sender::DatagramSenderHandle,
};
use crate::models::message::HomaMessage;
use crate::{
    actors::application_writer::ApplicationWriterHandle, constants::HOMA_SOCKET_PATH,
    models::registration::HomaRegistrationMessage,
};
use std::{
    collections::HashMap,
    fs,
    os::unix::net::{UnixListener, UnixStream},
    sync::{Arc, Mutex},
};
use tokio::sync::mpsc::{channel, Receiver, Sender};

pub struct ApplicationRegistrar {
    applications: Arc<Mutex<HashMap<u32, ApplicationHandle>>>,
    rx: Receiver<ApplicationRegistrarMessage>,
    application_registrar_handle: ApplicationRegistrarHandle,
    datagram_sender_handle: DatagramSenderHandle,
}

impl ApplicationRegistrar {
    fn handle_from_application_listener(&mut self, mut stream: UnixStream) {
        if let Ok(registration_message) = HomaRegistrationMessage::from_unix_stream(&mut stream) {
            println!("{:?}", registration_message);
            let read_stream = if let Ok(read_stream) = stream.try_clone() {
                read_stream
            } else {
                return;
            };
            let write_stream = if let Ok(write_stream) = stream.try_clone() {
                write_stream
            } else {
                return;
            };
            let mut applications = self.applications.lock().unwrap();
            let id = registration_message.application_id;
            if let None = applications.get(&id) {
                let application_writer_handle = ApplicationWriterHandle::new(write_stream);
                let application_handle = ApplicationHandle::new(
                    id,
                    application_writer_handle,
                    self.application_registrar_handle.clone(),
                    self.datagram_sender_handle.clone(),
                );
                applications.insert(id, application_handle.clone());

                ApplicationReader::start(read_stream, application_handle);
            }
        }
    }

    fn handle_from_application(&mut self, id: u32) {
        let mut applications = self.applications.lock().unwrap();
        applications.remove(&id);
    }
}

pub enum ApplicationRegistrarMessage {
    FromApplicationListener(UnixStream),
    FromApplication(u32),
}

fn run_application_registrar(mut application_registrar: ApplicationRegistrar) {
    use ApplicationRegistrarMessage::*;
    while let Some(application_registrar_message) = application_registrar.rx.blocking_recv() {
        match application_registrar_message {
            FromApplicationListener(stream) => {
                application_registrar.handle_from_application_listener(stream)
            }
            FromApplication(id) => application_registrar.handle_from_application(id),
        }
    }
}

#[derive(Clone)]
pub struct ApplicationRegistrarHandle {
    pub tx: Sender<ApplicationRegistrarMessage>,
}

impl ApplicationRegistrarHandle {
    pub fn new(
        applications: Arc<Mutex<HashMap<u32, ApplicationHandle>>>,
        datagram_sender_handle: DatagramSenderHandle,
    ) -> Self {
        let (tx, rx) = channel::<ApplicationRegistrarMessage>(100);
        let application_registrar_handle = Self { tx };
        let application_registrar = ApplicationRegistrar {
            applications,
            rx,
            application_registrar_handle: application_registrar_handle.clone(),
            datagram_sender_handle,
        };
        tokio::task::spawn_blocking(move || run_application_registrar(application_registrar));
        application_registrar_handle
    }
}
