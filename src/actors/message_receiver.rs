use super::{application::ApplicationHandle, datagram_sender::DatagramSenderHandle};
use crate::{
    constants::HOMA_DATAGRAM_PAYLOAD_LENGTH,
    models::{
        datagram::HomaDatagram,
        message::{HomaMessage, HomaMessageBuilder},
    },
};
use tokio::sync::mpsc::{channel, Receiver, Sender};

struct MessageReceiver {
    datagram_sender_handle: DatagramSenderHandle,
    application_handle: ApplicationHandle,
    rx: Receiver<HomaDatagram>,

    message_id: u64,
    source_address: [u8; 4],
    destination_address: [u8; 4],
    source_id: u32,
    destination_id: u32,
    expected: u64,
    collected: u64,
    content: Vec<u8>,
}

impl MessageReceiver {
    pub fn add_datagram(&mut self, datagram: &HomaDatagram) {
        self.collected += datagram.payload.len() as u64;
        self.content.append(&mut datagram.payload.clone());
    }

    pub fn priority(&self) -> u8 {
        let remainder = self.expected - self.collected;
        if remainder / HOMA_DATAGRAM_PAYLOAD_LENGTH as u64 > 255 {
            0
        } else {
            (255 - remainder / HOMA_DATAGRAM_PAYLOAD_LENGTH as u64) as u8
        }
    }

    pub fn check_message(&self) -> bool {
        self.collected >= self.expected
    }

    pub fn build_message(&self) -> HomaMessage {
        HomaMessageBuilder::default()
            .id(self.message_id)
            .source_address(self.source_address)
            .destination_address(self.destination_address)
            .source_id(self.source_id)
            .destination_id(self.destination_id)
            .content(self.content.clone())
            .build()
            .unwrap()
    }
}

async fn run_message_receiver(mut message_receiver: MessageReceiver) {
    use crate::actors::application::ApplicationMessage::ToApplicationWriter;
    println!("Started message receiver");
    if message_receiver.check_message() {
        let homa_message = message_receiver.build_message();
        message_receiver
            .application_handle
            .tx
            .send(ToApplicationWriter(homa_message))
            .await
            .unwrap();
        return;
    }
    while let Some(datagram) = message_receiver.rx.recv().await {
        println!("Received last rtt datagram");
        message_receiver.add_datagram(&datagram);
        let priority = message_receiver.priority();
        let grant = datagram.grant(priority).to_ipv4(priority);
        message_receiver
            .datagram_sender_handle
            .tx
            .send(grant)
            .await
            .unwrap();
        if message_receiver.check_message() {
            break;
        }
    }
    let homa_message = message_receiver.build_message();
    message_receiver
        .application_handle
        .tx
        .send(ToApplicationWriter(homa_message))
        .await
        .unwrap();
}

#[derive(Clone)]
pub struct MessageReceiverHandle {
    pub tx: Sender<HomaDatagram>,
}

impl MessageReceiverHandle {
    pub fn new(
        datagram: HomaDatagram,
        datagram_sender_handle: DatagramSenderHandle,
        application_handle: ApplicationHandle,
    ) -> Self {
        println!("{:?}", datagram);
        let (tx, rx) = channel(100);
        let message_receiver_actor = MessageReceiver {
            datagram_sender_handle,
            application_handle,
            rx,

            message_id: datagram.message_id,
            source_address: datagram.source_address,
            destination_address: datagram.destination_address,
            source_id: datagram.source_id,
            destination_id: datagram.destination_id,
            expected: datagram.message_length,
            collected: datagram.payload.len() as u64,
            content: datagram.payload,
        };
        tokio::spawn(run_message_receiver(message_receiver_actor));
        Self { tx }
    }
}
