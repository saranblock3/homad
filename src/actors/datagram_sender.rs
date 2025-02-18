use pnet::packet::ipv4::Ipv4Packet;
use pnet::transport::TransportSender;
use rand::Rng;
use std::ops::Range;
use std::thread;
use std::{net::IpAddr, time::Duration};
use tokio::sync::mpsc::error::SendError;
use tokio::sync::mpsc::{channel, Receiver, Sender};
use tokio::time::sleep;

struct DatagramSender {
    transport_sender: TransportSender,
    rx: Receiver<Vec<u8>>,
}

impl DatagramSender {
    fn handle_packet_bytes(&mut self, packet_bytes: Vec<u8>) {
        let packet = Ipv4Packet::new(&packet_bytes).unwrap();
        let address = packet.get_destination();
        let _ = self.transport_sender.send_to(packet, IpAddr::V4(address));
    }
}

fn run_datagram_sender(mut datagram_sender: DatagramSender) {
    while let Some(packet_bytes) = datagram_sender.rx.blocking_recv() {
        datagram_sender.handle_packet_bytes(packet_bytes);
    }
}

#[derive(Clone)]
pub struct DatagramSenderHandle {
    tx: Sender<Vec<u8>>,
}

impl DatagramSenderHandle {
    pub fn new(transport_sender: TransportSender) -> Self {
        let (tx, rx) = channel::<Vec<u8>>(3000000);
        let actor = DatagramSender {
            transport_sender,
            rx,
        };
        thread::spawn(move || run_datagram_sender(actor));
        Self { tx }
    }

    pub async fn send(&self, packet: Vec<u8>) -> Result<(), SendError<Vec<u8>>> {
        let timeout = rand::thread_rng().gen_range::<u64, Range<u64>>(0..200);
        sleep(Duration::from_micros(timeout)).await;
        self.tx.send(packet).await
    }
}
