use crate::models::message::HomaMessage;
use bincode::serialize;
use std::{io::Write, os::unix::net::UnixStream, thread};
use tokio::sync::mpsc::{channel, Receiver, Sender};

struct ApplicationWriter {
    stream: UnixStream,
    rx: Receiver<HomaMessage>,
}

impl ApplicationWriter {
    fn handle_message(&mut self, message: HomaMessage) -> Option<()> {
        if let Ok(message_bytes) = serialize(&message) {
            let mut message_payload = (message_bytes.len() as u64).to_le_bytes().to_vec();
            message_payload.append(&mut message_bytes.to_vec());
            if let Err(_) = self.stream.write_all(&message_payload) {
                return None;
            }
        }
        Some(())
    }
}

fn run_application_writer(mut application_writer: ApplicationWriter) {
    while let Some(message) = application_writer.rx.blocking_recv() {
        if let None = application_writer.handle_message(message) {
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
        let (tx, rx) = channel::<HomaMessage>(1000);
        let application_writer = ApplicationWriter { stream, rx };
        thread::spawn(move || {
            run_application_writer(application_writer);
        });
        Self { tx }
    }
}
