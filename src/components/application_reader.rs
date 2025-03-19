/*
ApplicationReader actor

This actor is bound to a specific application

It listens for messages from the application, deserializes them and passes them to the
Application actor

Upon detecting that the stream from the application is no longer readable,
it shuts down the write and read sides of the stream and informs the
Application actor
*/
use crate::components::application::ApplicationHandle;
use crate::components::application::ApplicationMessage::FromApplicationReader;
use crate::components::application::ApplicationMessage::Shutdown;
use crate::models::message::HomaMessage;
use async_std::os::unix::net::UnixStream as AsyncUnixStream;
use std::os::unix::net::UnixStream;
use tokio::task::JoinHandle;

pub struct ApplicationReader {
    stream: UnixStream,
    application_handle: ApplicationHandle,
}

impl ApplicationReader {
    // Spawn the ApplicationReader task as a future
    pub fn start(stream: UnixStream, application_handle: ApplicationHandle) -> JoinHandle<()> {
        let application_reader = ApplicationReader {
            stream,
            application_handle,
        };
        tokio::spawn(run_application_reader(application_reader))
    }
}

// Listen for messages, deserialize them and send them to the Application actor
async fn run_application_reader(application_reader: ApplicationReader) {
    let mut stream = AsyncUnixStream::from(application_reader.stream.try_clone().unwrap());
    loop {
        match HomaMessage::from_unix_stream(&mut stream).await {
            Ok(message_option) => {
                if let Some(message) = message_option {
                    application_reader
                        .application_handle
                        .send(FromApplicationReader(message))
                        .await
                        .expect("ApplicationReader -> Application failed");
                } else {
                    break;
                }
            }
            Err(_) => {
                break;
            }
        }
    }
    let _ = stream.shutdown(std::net::Shutdown::Both);
    application_reader
        .application_handle
        .send(Shutdown)
        .await
        .expect("ApplicationReader -> Application failed");
}
