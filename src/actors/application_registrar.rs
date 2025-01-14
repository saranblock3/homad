use super::{
    application::ApplicationHandle, application_reader::ApplicationReader,
    datagram_sender::DatagramSenderHandle,
};
use crate::{
    actors::application_writer::ApplicationWriterHandle, constants::HOMA_SOCKET_PATH,
    models::registration::HomaRegistrationMessage,
};
use std::{
    collections::HashMap,
    fs,
    os::unix::net::UnixListener,
    sync::{Arc, Mutex},
};

pub struct ApplicationRegistrar {
    listener: UnixListener,
    applications: Arc<Mutex<HashMap<u32, ApplicationHandle>>>,
    datagram_sender_handle: DatagramSenderHandle,
}

impl ApplicationRegistrar {
    pub fn start(
        applications: Arc<Mutex<HashMap<u32, ApplicationHandle>>>,
        datagram_sender_handle: DatagramSenderHandle,
    ) {
        fs::remove_file(HOMA_SOCKET_PATH);
        let listener = UnixListener::bind(HOMA_SOCKET_PATH).unwrap();
        let application_registrar = ApplicationRegistrar {
            listener,
            applications,
            datagram_sender_handle,
        };
        tokio::task::spawn_blocking(move || run_application_registrar(application_registrar));
    }
}

fn run_application_registrar(application_registrar: ApplicationRegistrar) {
    while let Ok((mut stream, _)) = application_registrar.listener.accept() {
        if let Ok(registration_message) = HomaRegistrationMessage::from_unix_stream(&mut stream) {
            println!("{:?}", registration_message);
            let read_stream = if let Ok(read_stream) = stream.try_clone() {
                read_stream
            } else {
                continue;
            };
            let write_stream = if let Ok(write_stream) = stream.try_clone() {
                write_stream
            } else {
                continue;
            };
            let mut applications = application_registrar.applications.lock().unwrap();
            let id = registration_message.application_id;
            if let None = applications.get(&id) {
                let application_writer_handle = ApplicationWriterHandle::new(write_stream);
                let application_handle = ApplicationHandle::new(
                    id,
                    application_writer_handle,
                    application_registrar.datagram_sender_handle.clone(),
                );
                applications.insert(id, application_handle.clone());

                ApplicationReader::start(read_stream, application_handle);
            }
        }
    }
}
