use super::application::{
    ApplicationHandle,
    ApplicationMessage::{FromApplicationReader, Shutdown},
};
use crate::models::message::HomaMessage;
use std::{os::unix::net::UnixStream, thread};

pub struct ApplicationReader {
    stream: UnixStream,
    application_handle: ApplicationHandle,
}

impl ApplicationReader {
    pub fn start(stream: UnixStream, application_handle: ApplicationHandle) {
        let application_reader = ApplicationReader {
            stream,
            application_handle,
        };
        thread::spawn(move || {
            run_application_reader(application_reader);
        });
    }
}

fn run_application_reader(mut application_reader: ApplicationReader) {
    while let Ok(message_option) = HomaMessage::from_unix_stream(&mut application_reader.stream) {
        if let Some(message) = message_option {
            println!(
                "APPLICATION READER (ID: {}) (ID: {}) READ MESSAGE",
                message.source_id, message.id
            );
            application_reader
                .application_handle
                .blocking_send(FromApplicationReader(message))
                .expect("ApplicationReader -> Application failed");
        }
    }
    let _ = application_reader.stream.shutdown(std::net::Shutdown::Both);
    application_reader
        .application_handle
        .blocking_send(Shutdown)
        .expect("ApplicationReader -> Application failed");
}
