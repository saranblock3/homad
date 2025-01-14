use super::application::ApplicationHandle;
use crate::models::datagram::HomaDatagram;
use bincode::deserialize;
use pnet::{
    packet::Packet,
    transport::{ipv4_packet_iter, TransportReceiver},
};
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    thread,
};

pub struct DatagramReceiver {
    applications: Arc<Mutex<HashMap<u32, ApplicationHandle>>>,
}

impl DatagramReceiver {
    fn handle_packet_payload(&self, packet_bytes: &[u8]) {
        use super::application::ApplicationMessage::FromDatagramReceiver;
        if let Ok(datagram) = deserialize::<HomaDatagram>(&packet_bytes) {
            let applications = self.applications.lock().unwrap();
            if let Some(application_handle) = applications.get(&datagram.destination_id) {
                application_handle
                    .tx
                    .blocking_send(FromDatagramReceiver(datagram))
                    .unwrap();
            }
        }
    }

    pub fn start(
        transport_receiver: TransportReceiver,
        applications: Arc<Mutex<HashMap<u32, ApplicationHandle>>>,
    ) {
        let datagram_receiver = DatagramReceiver { applications };
        thread::spawn(move || {
            run_datagram_receiver(datagram_receiver, transport_receiver);
        });
    }
}

fn run_datagram_receiver(
    datagram_receiver: DatagramReceiver,
    mut transport_receiver: TransportReceiver,
) {
    let mut packet_iter = ipv4_packet_iter(&mut transport_receiver);
    while let Ok((packet, _)) = packet_iter.next() {
        datagram_receiver.handle_packet_payload(packet.payload());
    }
}
