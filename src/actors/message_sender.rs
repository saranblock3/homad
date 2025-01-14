use super::datagram_sender::DatagramSenderHandle;
use crate::models::{datagram::HomaDatagram, message::HomaMessage};
use tokio::sync::mpsc::{channel, Receiver, Sender};

struct MessageSender {
    rx: Receiver<HomaDatagram>,
    message: HomaMessage,
    datagrams: Vec<HomaDatagram>,
    datagram_sender_handle: DatagramSenderHandle,
}

impl MessageSender {
    async fn handle_datagram(&self, datagram: HomaDatagram) {
        use crate::models::datagram::HomaDatagramType::*;
        match datagram.datagram_type {
            Grant => self.handle_grant(datagram).await,
            Resend => self.handle_resend(datagram).await,
            _ => (),
        }
    }

    async fn handle_grant(&self, grant_datagram: HomaDatagram) {
        if let Some(next_datagram) = self.datagrams.get(grant_datagram.sequence_number as usize) {
            let packet = next_datagram.to_ipv4(next_datagram.priority_control);
            self.datagram_sender_handle
                .tx
                .send(packet)
                .await
                .expect("MessageSender -> DatagramSender failed");
        }
    }

    async fn send_rtt_bytes(&self) {
        for i in 0..2 {
            if let Some(datagram) = self.datagrams.get(i) {
                let packet = datagram.to_ipv4(datagram.priority_control);
                self.datagram_sender_handle
                    .tx
                    .send(packet)
                    .await
                    .expect("MessageSender -> DatagramSender failed");
            }
        }
    }

    #[allow(unused)]
    async fn handle_resend(&self, datagram: HomaDatagram) {}
}

async fn run_message_sender(mut message_sender: MessageSender) {
    message_sender.send_rtt_bytes().await;
    println!("Sent rtt bytes");
    while let Some(datagram) = message_sender.rx.recv().await {
        message_sender.handle_datagram(datagram).await;
    }
}

#[derive(Clone)]
pub struct MessageSenderHandle {
    pub tx: Sender<HomaDatagram>,
}

impl MessageSenderHandle {
    pub fn new(message: HomaMessage, datagram_sender_handle: DatagramSenderHandle) -> Self {
        let (tx, rx) = channel::<HomaDatagram>(100);
        let datagrams = message.split();
        let message_sender = MessageSender {
            rx,
            message,
            datagrams,
            datagram_sender_handle,
        };
        tokio::spawn(run_message_sender(message_sender));
        Self { tx }
    }
}
