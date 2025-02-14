use crate::models::message::HomaMessage;
use async_std::{io::WriteExt, os::unix::net::UnixStream as AsyncUnixStream};
use bincode::serialize;
use std::os::unix::net::UnixStream;
use tokio::sync::mpsc::{channel, Receiver, Sender};

struct ApplicationWriter {
    stream: AsyncUnixStream,
    rx: Receiver<HomaMessage>,
}

impl ApplicationWriter {
    async fn handle_message(&mut self, message: HomaMessage) -> Option<()> {
        if let Ok(message_bytes) = serialize(&message) {
            let mut message_payload = (message_bytes.len() as u64).to_le_bytes().to_vec();
            message_payload.append(&mut message_bytes.to_vec());
            if let Err(_) = self.stream.write_all(&message_payload).await {
                return None;
            }
        }
        Some(())
    }
}

async fn run_application_writer(mut application_writer: ApplicationWriter) {
    while let Some(message) = application_writer.rx.recv().await {
        if let None = application_writer.handle_message(message).await {
            break;
        }
    }
}

#[derive(Clone)]
pub struct ApplicationWriterHandle {
    pub tx: Sender<HomaMessage>,
}

impl ApplicationWriterHandle {
    pub fn new(stream: UnixStream) -> Self {
        let stream = AsyncUnixStream::from(stream.try_clone().unwrap());
        let (tx, rx) = channel::<HomaMessage>(100000);
        let application_writer = ApplicationWriter { stream, rx };
        // thread::spawn(move || {
        //     run_application_writer(application_writer);
        // });
        tokio::spawn(run_application_writer(application_writer));
        Self { tx }
    }
}
