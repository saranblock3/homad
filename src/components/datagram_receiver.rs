/*
DatagramReceiver actor

Receive all incoming datagrams, pass them to the corresponding
Application/MessageSender/MessageReceiver
*/
use crate::components::application::ApplicationHandle;
use crate::models::datagram::HomaDatagram;
use bincode::deserialize;
use pnet::packet::Packet;
use pnet::transport::ipv4_packet_iter;
use pnet::transport::TransportReceiver;
use std::collections::HashMap;
use std::net::Ipv4Addr;
use std::sync::Arc;
use std::sync::Mutex;
use tokio;

pub struct DatagramReceiver {
    application_handles: Arc<Mutex<HashMap<u32, ApplicationHandle>>>,
}

impl DatagramReceiver {
    // Deserialize datagram, validate checksum and handle it
    fn handle_packet_payload(
        &self,
        packet_bytes: Vec<u8>,
        source_address: Ipv4Addr,
        destination_address: Ipv4Addr,
    ) {
        if let Ok(mut datagram) = deserialize::<HomaDatagram>(&packet_bytes) {
            if let Ok(checksum) = datagram.checksum() {
                if datagram.checksum == checksum {
                    self.handle_datagram(datagram, source_address, destination_address);
                }
            }
        }
    }

    // Send the datagram to the correct Application/MessageSender/MessageReceiver
    fn handle_datagram(
        &self,
        datagram: HomaDatagram,
        source_address: Ipv4Addr,
        destination_address: Ipv4Addr,
    ) {
        let applications = self.application_handles.lock().unwrap();
        if let Some(application_handle) = applications.get(&datagram.destination_id) {
            application_handle.blocking_send_datagram(
                datagram,
                source_address,
                destination_address,
            );
        }
    }

    // Start the DatagramReceiver
    #[allow(unused)]
    pub fn start(
        transport_receiver: TransportReceiver,
        application_handles: Arc<Mutex<HashMap<u32, ApplicationHandle>>>,
    ) {
        let datagram_receiver = DatagramReceiver {
            application_handles,
        };
        tokio::task::spawn_blocking(move || {
            run_datagram_receiver(datagram_receiver, transport_receiver);
        });
    }

    // Start many the DatagramReceivers
    pub fn start_many(
        transport_receiver: TransportReceiver,
        application_handles: Arc<Mutex<HashMap<u32, ApplicationHandle>>>,
    ) {
        let transport_receiver = Arc::new(Mutex::new(transport_receiver));
        for _ in 0..3 {
            let datagram_receiver = DatagramReceiver {
                application_handles: Arc::clone(&application_handles),
            };
            let transport_receiver = transport_receiver.clone();
            tokio::task::spawn_blocking(move || {
                run_datagram_receivers(datagram_receiver, transport_receiver);
            });
        }
    }
}

// Listen for inocming packets and handle packet payloads
fn run_datagram_receivers(
    datagram_receiver: DatagramReceiver,
    transport_receiver: Arc<Mutex<TransportReceiver>>,
) {
    loop {
        let packet_data = {
            let mut transport_receiver_guard = transport_receiver.lock().unwrap();
            let mut packet_iter = ipv4_packet_iter(&mut transport_receiver_guard);
            packet_iter.next().ok().and_then(|(packet, _)| {
                Some((
                    packet.payload().to_vec(),
                    packet.get_source(),
                    packet.get_destination(),
                ))
            })
        };
        if let Some((payload, source, destination)) = packet_data {
            datagram_receiver.handle_packet_payload(payload, source, destination);
        }
    }
}

// Listen for inocming packets and handle packet payloads
#[allow(unused)]
fn run_datagram_receiver(
    datagram_receiver: DatagramReceiver,
    mut transport_receiver: TransportReceiver,
) {
    let mut packet_iter = ipv4_packet_iter(&mut transport_receiver);
    while let Ok((packet, _)) = packet_iter.next() {
        datagram_receiver.handle_packet_payload(
            packet.payload().to_vec(),
            packet.get_source(),
            packet.get_destination(),
        );
    }
}
