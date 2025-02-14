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
        use crate::models::datagram::HomaDatagramType::*;
        println!("DATAGRAM RECEIVER === {:?}", packet_bytes[17..21].to_vec());
        if let Ok(datagram) = deserialize::<HomaDatagram>(&packet_bytes) {
            println!(
                "DATAGRAM RECEIVER RECEIVED DATAGRAM FROM (ID: {}) TO (ID: {}) WITH MESSAGE ID (ID: {})",
                datagram.source_id, datagram.destination_id, datagram.message_id
            );
            let applications = self.applications.lock().unwrap();
            if let Some(application_handle) = applications.get(&datagram.destination_id) {
                match datagram.datagram_type {
                    Data => {
                        if let Some((message_receiver_handle, _)) = application_handle
                            .message_receivers
                            .blocking_lock()
                            .get(&datagram.message_id)
                        {
                            // println!(
                            //     "DATAGRAM RECEIVER RECEIVED DATA DATAGRAM FROM (ID: {}) TO (ID: {}) WITH MESSAGE ID (ID: {}) DISPATCH TO MESSAGE RECEIVER",
                            //     datagram.source_id, datagram.destination_id, datagram.message_id
                            // );
                            let _ = message_receiver_handle.tx.blocking_send(datagram);
                        } else {
                            // println!(
                            //     "DATAGRAM RECEIVER RECEIVED DATA DATAGRAM FROM (ID: {}) TO (ID: {}) WITH MESSAGE ID (ID: {}) DISPATCH TO APPLICATION",
                            //     datagram.source_id, datagram.destination_id, datagram.message_id
                            // );
                            let _ =
                                application_handle.blocking_send(FromDatagramReceiver(datagram));
                        }
                    }
                    _ => {
                        if let Some((message_sender_handle, _)) = application_handle
                            .message_senders
                            .blocking_lock()
                            .get(&datagram.message_id)
                        {
                            let _ = message_sender_handle.tx.blocking_send(datagram);
                        }
                        // println!("DATAGRAM RECEIVER FAILED TO SEND");
                    }
                }
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
