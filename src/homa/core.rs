use crate::constants::*;
use crate::models::datagram::*;
use crate::models::message::*;
use crate::models::registration::*;
use bincode::{deserialize, serialize};
use nix::sys::socket::{self, sockopt::ReuseAddr, SockFlag, SockType, UnixAddr};
use pnet::packet::ip::IpNextHeaderProtocol;
use pnet::packet::ipv4::Ipv4Packet;
use pnet::packet::Packet;
use pnet::transport::TransportChannelType::Layer4;
use pnet::transport::TransportProtocol::Ipv4;
use pnet::transport::{ipv4_packet_iter, transport_channel, TransportReceiver, TransportSender};
use std::collections::HashMap;
use std::collections::HashSet;
use std::io::Write;
use std::io::{self};
use std::net::IpAddr;
use std::os::fd::AsFd;
use std::os::fd::AsRawFd;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::Path;
use std::sync::mpsc;
use std::sync::mpsc::{Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::{fs, thread};

pub struct Homa {
    application_ids: HashSet<u32>,
    receiver_application_handler_channels: Mutex<HashMap<u32, Sender<HomaDatagram>>>,
    receiver_message_handler_channels: Mutex<HashMap<u64, Sender<HomaDatagram>>>,
    transmitter_message_handler_channels: Mutex<HashMap<u64, Sender<HomaDatagram>>>,
    outgoing_datagram_transmitter_tx: Sender<Vec<u8>>,
    outgoing_datagram_transmitter_rx: Mutex<Receiver<Vec<u8>>>,
}

impl Homa {
    pub fn new() -> Arc<Self> {
        let (outgoing_datagram_transmitter_tx, outgoing_datagram_transmitter_rx) =
            mpsc::channel::<Vec<u8>>();
        Arc::new(Homa {
            application_ids: HashSet::new(),
            receiver_application_handler_channels: Mutex::new(HashMap::new()),
            receiver_message_handler_channels: Mutex::new(HashMap::new()),
            transmitter_message_handler_channels: Mutex::new(HashMap::new()),
            outgoing_datagram_transmitter_tx,
            outgoing_datagram_transmitter_rx: Mutex::new(outgoing_datagram_transmitter_rx),
        })
    }

    pub fn start(self: Arc<Self>) -> Result<(), io::Error> {
        match transport_channel(4096, Layer4(Ipv4(IpNextHeaderProtocol(146)))) {
            Ok((outgoing_socket, incoming_socket)) => {
                let homa = Arc::clone(&self);
                thread::spawn(move || {
                    homa.incoming_datagram_listener(incoming_socket);
                });

                let homa = Arc::clone(&self);
                thread::spawn(move || {
                    homa.outgoing_datagram_transmitter(outgoing_socket);
                });

                self.application_listener();
                Ok(())
            }
            Err(err) => Err(err),
        }
    }

    pub fn incoming_datagram_listener(self: Arc<Self>, mut incoming_socket: TransportReceiver) {
        let mut ipv4_packet_iter = ipv4_packet_iter(&mut incoming_socket);
        while let Ok((ipv4_packet, _)) = ipv4_packet_iter.next() {
            if let Ok(homa_datagram) = deserialize::<HomaDatagram>(ipv4_packet.payload()) {
                match homa_datagram.datagram_type {
                    HomaDatagramType::Data => {
                        Arc::clone(&self).handle_incoming_data_datagram(homa_datagram)
                    }
                    HomaDatagramType::Grant => {
                        Arc::clone(&self).handle_incoming_control_datagram(homa_datagram)
                    }
                    _ => todo!("handle other datagram types"),
                }
            }
        }
    }

    pub fn handle_incoming_data_datagram(self: Arc<Self>, homa_datagram: HomaDatagram) {
        {
            let receiver_message_handler_channels =
                self.receiver_message_handler_channels.lock().unwrap();
            if let Some(tx) = receiver_message_handler_channels.get(&homa_datagram.message_id) {
                let _ = tx.send(homa_datagram);
                return;
            }
        }
        {
            let receiver_application_handler_channels =
                self.receiver_application_handler_channels.lock().unwrap();
            if let Some(tx) =
                receiver_application_handler_channels.get(&homa_datagram.destination_id)
            {
                tx.send(homa_datagram).unwrap();
                return;
            }
        }
    }

    pub fn handle_incoming_control_datagram(self: Arc<Self>, homa_datagram: HomaDatagram) {
        let transmitter_message_handler_channels =
            self.transmitter_message_handler_channels.lock().unwrap();
        if let Some(tx) = transmitter_message_handler_channels.get(&homa_datagram.message_id) {
            tx.send(homa_datagram).unwrap();
        }
    }

    pub fn application_listener(self: Arc<Self>) {
        if Path::new(HOMA_SOCKET_PATH).exists() {
            fs::remove_file(HOMA_SOCKET_PATH).unwrap();
        }
        let listener = UnixListener::bind(HOMA_SOCKET_PATH).unwrap();
        let mut listener_iter = listener.incoming();

        while let Some(Ok(mut stream)) = listener_iter.next() {
            if let Ok(registration_message) = HomaRegistrationMessage::from_unix_stream(&mut stream)
            {
                if self
                    .application_ids
                    .contains(&registration_message.application_id)
                {
                    continue;
                }

                let mut receiver_application_handler_channels =
                    self.receiver_application_handler_channels.lock().unwrap();

                let (receiver_application_handler_tx, receiver_application_handler_rx) =
                    mpsc::channel::<HomaDatagram>();
                receiver_application_handler_channels.insert(
                    registration_message.application_id,
                    receiver_application_handler_tx,
                );

                let (tx, rx) = mpsc::channel();

                let self_clone = Arc::clone(&self);
                thread::spawn(move || {
                    self_clone.receiver_application_handler(receiver_application_handler_rx, tx);
                });

                let stream_clone = stream.try_clone().unwrap();
                let self_clone = Arc::clone(&self);
                thread::spawn(move || {
                    self_clone.receiver_application_writer(stream_clone, rx);
                });

                let self_clone = Arc::clone(&self);
                thread::spawn(move || self_clone.transmitter_application_handler(stream));
            }
        }
    }

    pub fn outgoing_datagram_transmitter(self: Arc<Self>, mut outgoing_socket: TransportSender) {
        let outgoing_datagram_transmitter_rx =
            self.outgoing_datagram_transmitter_rx.lock().unwrap();
        for packet in outgoing_datagram_transmitter_rx.iter() {
            let packet = Ipv4Packet::new(&packet).unwrap();
            let address = packet.get_destination();
            let _ = outgoing_socket.send_to(packet, IpAddr::V4(address));
        }
    }

    pub fn receiver_application_handler(
        self: Arc<Self>,
        receiver_application_handler_rx: Receiver<HomaDatagram>,
        receiver_application_writer_tx: Sender<HomaMessage>,
    ) {
        for datagram in receiver_application_handler_rx {
            let mut receiver_message_handler_channels =
                self.receiver_message_handler_channels.lock().unwrap();
            let handler_tx = receiver_message_handler_channels.get(&datagram.message_id);
            if let Some(handler_tx) = handler_tx {
                handler_tx.send(datagram).unwrap();
            } else {
                let (handler_tx, handler_rx) = mpsc::channel::<HomaDatagram>();
                receiver_message_handler_channels.insert(datagram.message_id, handler_tx.clone());
                let outgoing_datagram_transmitter_tx =
                    self.outgoing_datagram_transmitter_tx.clone();
                let self_clone = Arc::clone(&self);
                let receiver_application_writer_tx = receiver_application_writer_tx.clone();
                thread::spawn(move || {
                    self_clone.receiver_message_handler(
                        handler_rx,
                        datagram,
                        outgoing_datagram_transmitter_tx,
                        receiver_application_writer_tx,
                    )
                });
            }
        }
    }

    pub fn receiver_application_writer(
        self: Arc<Self>,
        mut stream: UnixStream,
        rx: Receiver<HomaMessage>,
    ) {
        for homa_message in rx {
            println!("||||||||||||||| {:?}", homa_message.source_address);
            let homa_message_bytes = serialize(&homa_message).unwrap();
            let mut size = (homa_message_bytes.len() as u64).to_le_bytes().to_vec();
            size.append(&mut homa_message_bytes.to_vec());
            let _ = stream.write_all(&size);
        }
    }

    pub fn transmitter_application_handler(self: Arc<Self>, mut stream: UnixStream) {
        while let Some(homa_message) = HomaMessage::from_unix_stream(&mut stream) {
            let mut message_handler_channels =
                self.transmitter_message_handler_channels.lock().unwrap();
            let (message_handler_tx, transmitter_message_handler_rx) =
                mpsc::channel::<HomaDatagram>();
            message_handler_channels.insert(homa_message.id, message_handler_tx);

            let outgoing_datagram_transmitter_tx = self.outgoing_datagram_transmitter_tx.clone();

            let self_clone = Arc::clone(&self);

            thread::spawn(move || {
                self_clone.transmitter_message_handler(
                    homa_message,
                    transmitter_message_handler_rx,
                    outgoing_datagram_transmitter_tx,
                )
            });
        }
    }

    pub fn receiver_message_handler(
        self: Arc<Self>,
        rx: Receiver<HomaDatagram>,
        first_datagram: HomaDatagram,
        outgoing_datagram_transmitter_tx: Sender<Vec<u8>>,
        receiver_application_writer_tx: Sender<HomaMessage>,
    ) {
        println!("{:?}", first_datagram);
        let mut datagram_collector = HomaDatagramCollector::new(&first_datagram);
        let priority = datagram_collector.priority();
        let grant = first_datagram.grant(priority).to_ipv4(priority);
        outgoing_datagram_transmitter_tx.send(grant).unwrap();
        for datagram in rx {
            println!("{:?}", datagram);
            datagram_collector.add(&datagram);
            let priority = datagram_collector.priority();
            let grant = datagram.grant(priority).to_ipv4(priority);
            outgoing_datagram_transmitter_tx.send(grant).unwrap();
            if datagram_collector.is_complete() {
                break;
            }
        }
        let homa_message = datagram_collector.to_message();
        receiver_application_writer_tx.send(homa_message).unwrap();
    }

    pub fn transmitter_message_handler(
        self: Arc<Self>,
        homa_message: HomaMessage,
        rx: Receiver<HomaDatagram>,
        outgoing_datagram_transmitter_tx: Sender<Vec<u8>>,
    ) {
        let datagrams = homa_message.split();
        let packet = datagrams.get(0).unwrap().to_ipv4(255);
        outgoing_datagram_transmitter_tx.send(packet).unwrap();

        for datagram in rx {
            match datagram.datagram_type {
                HomaDatagramType::Grant => {
                    if let Some(next_datagram) = datagrams.get(
                        datagram.sequence_number as usize / HOMA_DATAGRAM_PAYLOAD_LENGTH as usize,
                    ) {
                        let packet = next_datagram.to_ipv4(datagram.priority_control);
                        outgoing_datagram_transmitter_tx.send(packet).unwrap();
                    } else {
                        return;
                    }
                }
                _ => {}
            }
        }
    }
}
