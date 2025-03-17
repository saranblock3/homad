/*
DatagamSender

This component used to be an actor but testing showed that it
is far more efficient for other actors to directly access the service
of this component by acquiring a lock to the transport_sender and
sending packets directly
*/
use pnet::packet::ipv4::Ipv4Packet;
use pnet::transport::TransportSender;
use std::io;
use std::net::IpAddr;
use std::sync::Arc;
use tokio::sync::Mutex;

#[derive(Clone)]
pub struct DatagramSenderHandle {
    transport_sender: Arc<Mutex<TransportSender>>,
}

impl DatagramSenderHandle {
    // Take the transport_sender channel as input and initialize the
    // DatagramSenderHandle with it
    pub fn new(transport_sender: TransportSender) -> Self {
        Self {
            transport_sender: Arc::new(Mutex::new(transport_sender)),
        }
    }

    // Assemble the IP datagram and send to the destination address
    pub async fn send(&self, packet: Vec<u8>) -> io::Result<usize> {
        let packet = Ipv4Packet::new(&packet).unwrap();
        let address = packet.get_destination();
        let mut transport_sender = self.transport_sender.lock().await;
        transport_sender.send_to(packet, IpAddr::V4(address))
    }
}
