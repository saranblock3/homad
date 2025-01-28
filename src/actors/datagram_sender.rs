use pnet::packet::ipv4::Ipv4Packet;
use pnet::transport::TransportSender;
use std::net::IpAddr;
use std::thread;
use tokio::sync::mpsc::{channel, Receiver, Sender};

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
    pub tx: Sender<Vec<u8>>,
}

impl DatagramSenderHandle {
    pub fn new(transport_sender: TransportSender) -> Self {
        let (tx, rx) = channel::<Vec<u8>>(10000);
        let actor = DatagramSender {
            transport_sender,
            rx,
        };
        thread::spawn(move || run_datagram_sender(actor));
        Self { tx }
    }
}
