/*
ApplicationWriter actor

This actor is responsible for listening for received messages,
serializing them and delivering them to the application via the stream
*/
use crate::models::message::HomaMessage;
use async_std::io::WriteExt;
use async_std::os::unix::net::UnixStream as AsyncUnixStream;
use bincode::serialize;
use std::os::unix::net::UnixStream;
use tokio::sync::mpsc::channel;
use tokio::sync::mpsc::Receiver;
use tokio::sync::mpsc::Sender;
use tokio::task::JoinHandle;

struct ApplicationWriter {
    stream: AsyncUnixStream,
    rx: Receiver<HomaMessage>,
}

impl ApplicationWriter {
    // Serialize and write the message to the stream
    async fn handle_message(&mut self, message: HomaMessage) -> Option<()> {
        if let Ok(message_bytes) = serialize(&message) {
            let mut message_payload = (message_bytes.len() as u64).to_le_bytes().to_vec();
            message_payload.append(&mut message_bytes.to_vec());
            println!("Message payload size: {}", message_payload.len());
            if let Err(_) = self.stream.write_all(&message_payload).await {
                return None;
            }
        }
        Some(())
    }
}

// Receive messages from the receiving channel and handle it
async fn run_application_writer(mut application_writer: ApplicationWriter) {
    while let Some(message) = application_writer.rx.recv().await {
        if application_writer.handle_message(message).await == None {
            break;
        }
    }
    println!("APPLICATION WRITER CLOSED");
}

#[derive(Clone)]
pub struct ApplicationWriterHandle {
    pub tx: Sender<HomaMessage>,
}

impl ApplicationWriterHandle {
    // Start the ApplicationWriter, return the actor handle and join handle
    pub fn new(stream: UnixStream) -> (Self, JoinHandle<()>) {
        let stream = AsyncUnixStream::from(stream.try_clone().unwrap());
        let (tx, rx) = channel::<HomaMessage>(1000);
        let application_writer = ApplicationWriter { stream, rx };
        let join_handle = tokio::spawn(run_application_writer(application_writer));
        (Self { tx }, join_handle)
    }
}
