use std::fs;
use std::os::unix::net::UnixListener;
use crate::actors::application_registrar::ApplicationRegistrarMessage::FromApplicationListener;
use crate::actors::application_registrar::ApplicationRegistrarHandle;
use crate::constants::HOMA_SOCKET_PATH;

pub struct ApplicationListener {
    listener: UnixListener,
    application_registrar_handle: ApplicationRegistrarHandle
}

impl ApplicationListener {
    pub fn start(application_registrar_handle: ApplicationRegistrarHandle) {
        fs::remove_file(HOMA_SOCKET_PATH);
        let listener = UnixListener::bind(crate::constants::HOMA_SOCKET_PATH).unwrap();
        let application_listener = Self { listener, application_registrar_handle };
        tokio::task::spawn_blocking(move || run_application_listener(application_listener));
    }
}

fn run_application_listener(mut application_listener: ApplicationListener) {
    while let Ok((stream, _)) = application_listener.listener.accept() {
        application_listener.application_registrar_handle.tx.blocking_send(FromApplicationListener(stream)).unwrap()
    }
}

