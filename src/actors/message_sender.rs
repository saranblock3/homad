use super::application::ApplicationHandle;
use super::datagram_sender::DatagramSenderHandle;
use super::priority_manager::PriorityManagerHandle;
use crate::constants::UNSCHEDULED_HOMA_DATAGRAM_LIMIT;
use crate::models::datagram::HomaDatagramType::Grant;
use crate::models::{datagram::HomaDatagram, message::HomaMessage};
use tokio::select;
use tokio::sync::mpsc::{channel, Receiver, Sender};
use tokio::task::JoinHandle;
use tokio::time::{sleep, Duration};

struct MessageSender {
    rx: Receiver<HomaDatagram>,
    message: HomaMessage,
    datagrams: Vec<HomaDatagram>,
    application_handle: ApplicationHandle,
    priority_manager_handle: PriorityManagerHandle,
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
            let packet = next_datagram.to_ipv4(next_datagram.priority);
            self.datagram_sender_handle
                .tx
                .send(packet)
                .await
                .expect("MessageSender -> DatagramSender failed");
        }
    }

    async fn handle_resend(&self, resend_datagram: HomaDatagram) {
        if let Some(next_datagram) = self.datagrams.get(resend_datagram.sequence_number as usize) {
            let packet = next_datagram.to_ipv4(resend_datagram.priority);
            self.datagram_sender_handle
                .tx
                .send(packet)
                .await
                .expect("MessageSender -> DatagramSender failed");
        }
    }

    fn check_message(&self, datagram: &HomaDatagram) -> bool {
        if datagram.sequence_number == self.datagrams.len() as u64 {
            return true;
        }
        false
    }

    async fn send_datagram_slice(&mut self, start: usize, end: usize) {
        for i in start..end {
            if let Some(datagram) = self.datagrams.get(i) {
                let packet = datagram.to_ipv4(64);
                self.datagram_sender_handle
                    .tx
                    .send(packet)
                    .await
                    .expect("MessageSender -> DatagramSender failed");
            }
        }
    }

    async fn send_unscheduled_datagrams(&mut self) -> Option<HomaDatagram> {
        self.send_datagram_slice(0, UNSCHEDULED_HOMA_DATAGRAM_LIMIT)
            .await;
        for i in 1..=5 {
            select! {
                _ = sleep(Duration::from_millis(400*i)) => {
                    self.send_datagram_slice(0, UNSCHEDULED_HOMA_DATAGRAM_LIMIT)
                        .await;
                }
                Some(datagram) = self.rx.recv() => {
                    return Some(datagram);
                }
            }
        }
        None
    }

    async fn send_requested_datagrams(&mut self, datagram: HomaDatagram) {
        self.handle_datagram(datagram).await;
        loop {
            select! {
                _ = sleep(Duration::from_millis(800)) => {
                    return;
                }
                Some(datagram) = self.rx.recv() => {
                    if self.check_message(&datagram) {
                        return
                    }
                    self.handle_datagram(datagram).await;
                }
            }
        }
    }
}

async fn run_message_sender(mut message_sender: MessageSender) {
    if let Some(datagram) = message_sender.send_unscheduled_datagrams().await {
        message_sender.send_requested_datagrams(datagram).await;
    }
}

#[derive(Clone)]
pub struct MessageSenderHandle {
    pub tx: Sender<HomaDatagram>,
}

impl MessageSenderHandle {
    pub fn new(
        message: HomaMessage,
        application_handle: ApplicationHandle,
        priority_manager_handle: PriorityManagerHandle,
        datagram_sender_handle: DatagramSenderHandle,
    ) -> (Self, JoinHandle<()>) {
        let (tx, rx) = channel::<HomaDatagram>(1000);
        let datagrams = message.split();
        let message_sender = MessageSender {
            rx,
            message,
            datagrams,
            application_handle,
            priority_manager_handle,
            datagram_sender_handle,
        };
        let join_handle = tokio::spawn(run_message_sender(message_sender));
        (Self { tx }, join_handle)
    }
}
