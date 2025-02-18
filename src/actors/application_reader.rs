use super::application::{
    ApplicationHandle,
    ApplicationMessage::{FromApplicationReader, Shutdown},
};
use crate::models::message::HomaMessage;
use async_std::os::unix::net;
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
        // thread::spawn(move || {
        //     run_application_reader(application_reader);
        // });
        tokio::spawn(run_application_reader(application_reader));
    }
}

async fn run_application_reader(application_reader: ApplicationReader) {
    let mut stream = net::UnixStream::from(application_reader.stream.try_clone().unwrap());
    while let Ok(message_option) = HomaMessage::from_unix_stream(&mut stream).await {
        if let Some(message) = message_option {
            application_reader
                .application_handle
                .send(FromApplicationReader(message))
                .await
                .expect("ApplicationReader -> Application failed");
        }
    }
    // let _ = application_reader.stream.shutdown(std::net::Shutdown::Both);
    let _ = stream.shutdown(std::net::Shutdown::Both);
    application_reader
        .application_handle
        .send(Shutdown)
        .await
        .expect("ApplicationReader -> Application failed");
}
