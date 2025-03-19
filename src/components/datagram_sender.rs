use libc::printf;
/*
DatagamSender

This component used to be an actor but testing showed that it
is far more efficient for other actors to directly access the service
of this component by acquiring a lock to the transport_sender and
sending packets directly
*/
use pnet::packet::ip::IpNextHeaderProtocol;
use pnet::packet::ipv4::Ipv4Packet;
use pnet::transport::transport_channel;
use pnet::transport::TransportChannelType::Layer4;
use pnet::transport::TransportProtocol::Ipv4;
use pnet::transport::TransportSender;
use rand::Rng;
use std::net::IpAddr;
use std::ops::Range;
use std::sync::Arc;
use std::{io, thread};
use tokio::sync::Mutex;

#[derive(Clone)]
pub struct DatagramSenderHandle {
    transport_senders: Vec<Arc<Mutex<TransportSender>>>,
}

impl DatagramSenderHandle {
    // Take the transport_sender channel as input and initialize the
    // DatagramSenderHandle with it
    pub fn new(transport_sender: TransportSender) -> Self {
        let mut transport_senders = Vec::new();
        for _ in 0..30 {
            if let Ok((transport_sender, _)) =
                transport_channel(300000, Layer4(Ipv4(IpNextHeaderProtocol(146))))
            {
                transport_senders.push(Arc::new(Mutex::new(transport_sender)));
            }
        }
        Self { transport_senders }
    }

    // Assemble the IP datagram and send to the destination address
    pub async fn send(&self, packet: Vec<u8>) -> io::Result<usize> {
        let packet = Ipv4Packet::new(&packet).unwrap();
        let address = packet.get_destination();
        let i =
            rand::thread_rng().gen_range::<usize, Range<usize>>(0..self.transport_senders.len());
        let mut transport_sender = self.transport_senders[i].lock().await;
        transport_sender.send_to(packet, IpAddr::V4(address))
    }
}
