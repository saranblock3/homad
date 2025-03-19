/*
ApplicationListener actor

This actor initializes the homa unix socket at the specified path

The actor then listens for and accepts any applications wishing to connect

The actor then passes the created stream to the ApplicationRegistrar
*/
use crate::components::application_registrar::ApplicationRegistrarHandle;
use crate::components::application_registrar::ApplicationRegistrarMessage::FromApplicationListener;
use crate::config::CONFIG;
use std::fs;
use std::os::unix::net::UnixListener;

pub struct ApplicationListener {
    // Listener to accept new applications
    listener: UnixListener,
    // Handle to contact ApplicationRegistrar
    application_registrar_handle: ApplicationRegistrarHandle,
}

impl ApplicationListener {
    // Remove the existing homa unix socket if one exists,
    // then create and bind the UnixListener
    fn init() -> Result<UnixListener, String> {
        let _ = fs::remove_file(CONFIG.SOCKET_PATH.clone());
        UnixListener::bind(CONFIG.SOCKET_PATH.clone())
            .map_err(|e| format!("ApplicationListener failed to start UnixListener: {}", e))
    }

    // Spawn the ApplicationListener task as a dedicated thread
    pub fn start(application_registrar_handle: ApplicationRegistrarHandle) -> Result<(), String> {
        let listener = Self::init()?;
        let application_listener = Self {
            listener,
            application_registrar_handle,
        };
        tokio::task::spawn_blocking(move || run_application_listener(application_listener));
        Ok(())
    }
}

// Listen for and accept applications wish to connect,
// send the stream to the ApplicationRegistrar
fn run_application_listener(mut application_listener: ApplicationListener) {
    loop {
        while let Ok((stream, _)) = application_listener.listener.accept() {
            application_listener
                .application_registrar_handle
                .blocking_send(FromApplicationListener(stream))
                .expect("ApplicationListener -> ApplicationRegistrar failed");
        }
        application_listener.listener = ApplicationListener::init()
            .expect("ApplicationListener failed to restart UnixListener");
    }
}
