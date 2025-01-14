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
    while let Some(message) = HomaMessage::from_unix_stream(&mut application_reader.stream) {
        application_reader
            .application_handle
            .tx
            .blocking_send(FromApplicationReader(message))
            .unwrap();
    }
    application_reader.stream.shutdown(std::net::Shutdown::Both);
    application_reader
        .application_handle
        .tx
        .blocking_send(Shutdown)
        .unwrap();
}
