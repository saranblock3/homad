use crate::constants::*;
use crate::models::datagram;
use crate::models::datagram::*;
use crate::models::message::*;
use crate::models::registration::*;
use crate::utils::{ipv4_bytes_to_string, ipv4_string_to_bytes};
use bincode::{deserialize, serialize};
use pnet::packet::ip::IpNextHeaderProtocol;
use pnet::packet::ipv4::Ipv4Packet;
use pnet::packet::Packet;
use pnet::transport::TransportChannelType::Layer4;
use pnet::transport::TransportProtocol::Ipv4;
use pnet::transport::{ipv4_packet_iter, transport_channel, TransportReceiver, TransportSender};
use std::collections::HashMap;
use std::collections::HashSet;
use std::io::{self};
use std::net::IpAddr;
use std::net::Ipv4Addr;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::Path;
use std::str::FromStr;
use std::sync::mpsc;
use std::sync::mpsc::{Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread::sleep;
use std::time::Duration;
use std::{fs, thread};

pub struct Homa {
    application_sockets: Mutex<HashMap<u32, Arc<UnixStream>>>,
    receiver_application_handler_channels: Mutex<HashMap<u32, Sender<HomaDatagram>>>,
    receiver_message_handler_channels: Mutex<HashMap<u64, Sender<HomaDatagram>>>,
    transmitter_message_handler_channels: Mutex<HashMap<u64, Sender<HomaDatagram>>>,
    sockets: (Mutex<TransportSender>, Mutex<TransportReceiver>),
    outgoing_datagram_transmitter_tx: Sender<Vec<u8>>,
    outgoing_datagram_transmitter_rx: Mutex<Receiver<Vec<u8>>>,
}

impl Homa {
    pub fn new() -> Result<Self, io::Error> {
        let (outgoing_datagram_tx, outgoing_datagram_rx) = mpsc::channel::<Vec<u8>>();
        match transport_channel(4096, Layer4(Ipv4(IpNextHeaderProtocol(146)))) {
            Ok(sockets) => Ok(Homa {
                application_sockets: Mutex::new(HashMap::new()),
                receiver_application_handler_channels: Mutex::new(HashMap::new()),
                receiver_message_handler_channels: Mutex::new(HashMap::new()),
                transmitter_message_handler_channels: Mutex::new(HashMap::new()),
                sockets: (Mutex::new(sockets.0), Mutex::new(sockets.1)),
                outgoing_datagram_transmitter_tx: outgoing_datagram_tx,
                outgoing_datagram_transmitter_rx: Mutex::new(outgoing_datagram_rx),
            }),
            Err(err) => Err(err),
        }
    }

    pub fn incoming_datagram_listener(self: Arc<Self>) {
        let mut transport_receiver = self.sockets.1.lock().unwrap();
        let mut ipv4_packet_iter = ipv4_packet_iter(&mut transport_receiver);
        loop {
            match ipv4_packet_iter.next() {
                Ok((packet, _)) => {
                    println!("============= {:?}", packet);
                    let homa_datagram = deserialize::<HomaDatagram>(packet.payload()).unwrap();
                    match homa_datagram.datagram_type {
                        HomaDatagramType::Data => {
                            Arc::clone(&self).handle_incoming_data_datagram(homa_datagram)
                        }
                        _ => Arc::clone(&self).handle_incoming_control_datagram(homa_datagram),
                    }
                }
                Err(e) => {
                    eprintln!("Error receiving packet: {}", e);
                }
            }
        }
    }

    pub fn handle_incoming_data_datagram(self: Arc<Self>, homa_datagram: HomaDatagram) {
        {
            let receiver_message_handler_channels =
                self.receiver_message_handler_channels.lock().unwrap();
            if let Some(tx) = receiver_message_handler_channels.get(&homa_datagram.message_id) {
                tx.send(homa_datagram).unwrap();
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
        println!("application does not exist");
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

        for stream in listener.incoming() {
            match stream {
                Ok(mut stream) => match HomaRegistrationMessage::from_unix_stream(&mut stream) {
                    Ok(registration_message) => {
                        {
                            let mut applications = self.application_sockets.lock().unwrap();
                            if applications.contains_key(&registration_message.application_id) {
                                println!("application id in use");
                                continue;
                            }
                        }
                        println!("{:?}", registration_message);
                        {
                            let mut receiver_application_handler_channels =
                                self.receiver_application_handler_channels.lock().unwrap();

                            let (receiver_application_handler_tx, receiver_application_handler_rx) =
                                mpsc::channel::<HomaDatagram>();
                            receiver_application_handler_channels.insert(
                                registration_message.application_id,
                                receiver_application_handler_tx,
                            );

                            let stream_clone = stream.try_clone().unwrap();
                            let self_clone = Arc::clone(&self);
                            thread::spawn(move || {
                                self_clone.receiver_application_handler(
                                    stream_clone,
                                    receiver_application_handler_rx,
                                );
                            });

                            let self_clone = Arc::clone(&self);
                            thread::spawn(move || {
                                self_clone.transmitter_application_handler(stream)
                            });
                        }
                    }
                    Err(_) => {
                        continue;
                    }
                },
                Err(err) => eprintln!("application client registration failed: {:?}", err),
            }
        }
    }

    pub fn outgoing_datagram_transmitter(self: Arc<Self>) {
        println!("started local application_listener");
        let outgoing_datagram_transmitter_rx =
            self.outgoing_datagram_transmitter_rx.lock().unwrap();
        let mut outgoing_socket = self.sockets.0.lock().unwrap();
        for packet in outgoing_datagram_transmitter_rx.iter() {
            let packet = Ipv4Packet::new(&packet).unwrap();
            let address = packet.get_destination();
            outgoing_socket
                .send_to(packet, IpAddr::V4(address))
                .unwrap();
            println!("Sent datagram");
        }
    }

    pub fn receiver_application_handler(
        self: Arc<Self>,
        mut stream: UnixStream,
        receiver_application_handler_rx: Receiver<HomaDatagram>,
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
                thread::spawn(move || {
                    self_clone
                        .receiver_message_handler(handler_rx, outgoing_datagram_transmitter_tx)
                });
                handler_tx.send(datagram).unwrap();
            }
        }
    }

    pub fn transmitter_application_handler(self: Arc<Self>, mut stream: UnixStream) {
        println!("started application handler");
        loop {
            if let Ok(homa_message) = HomaMessage::from_unix_stream(&mut stream) {
                let mut message_handler_channels =
                    self.transmitter_message_handler_channels.lock().unwrap();
                let (message_handler_tx, message_handler_rx) = mpsc::channel::<HomaDatagram>();
                message_handler_channels.insert(homa_message.id, message_handler_tx);
                let outgoing_datagram_transmitter_tx =
                    self.outgoing_datagram_transmitter_tx.clone();
                let self_clone = Arc::clone(&self);
                thread::spawn(move || {
                    self_clone.transmitter_message_handler(
                        homa_message,
                        message_handler_rx,
                        outgoing_datagram_transmitter_tx,
                    )
                });
            }
        }
    }

    pub fn receiver_message_handler(
        self: Arc<Self>,
        rx: Receiver<HomaDatagram>,
        outgoing_datagram_transmitter_tx: Sender<Vec<u8>>,
    ) {
        let mut datagrams = Vec::<HomaDatagram>::new();
        for datagram in rx {
            let grant = datagram.grant();
            let index = datagram.sequence_number / HOMA_DATAGRAM_PAYLOAD_LENGTH as u64;
            let message_length = datagram.message_length as usize;
            if datagrams.len() > index as usize {
                datagrams[index as usize] = datagram;
            } else if datagrams.len() == index as usize {
                datagrams.push(datagram);
            }
            if datagrams.len() > message_length as usize / HOMA_DATAGRAM_PAYLOAD_LENGTH as usize {
                break;
            }
            let packet = grant.to_ipv4(
                Ipv4Addr::from_str(&ipv4_bytes_to_string(&grant.source_address)).unwrap(),
                Ipv4Addr::from_str(&ipv4_bytes_to_string(&grant.destination_address)).unwrap(),
                255,
            );
            outgoing_datagram_transmitter_tx.send(packet).unwrap();
            println!("Sent grant")
        }
        let message_content = datagrams
            .iter()
            .map(|datagram| datagram.payload.clone())
            .flatten()
            .collect::<Vec<u8>>();
        println!("{}", String::from_utf8(message_content).unwrap());
    }

    pub fn transmitter_message_handler(
        self: Arc<Self>,
        homa_message: HomaMessage,
        rx: Receiver<HomaDatagram>,
        outgoing_datagram_transmitter_tx: Sender<Vec<u8>>,
    ) {
        println!("started message handler: {:?}", homa_message);
        let datagrams = homa_message.split();
        let packet = datagrams.get(0).unwrap().to_ipv4(
            Ipv4Addr::from_str(&ipv4_bytes_to_string(&homa_message.source_address)).unwrap(),
            Ipv4Addr::from_str(&ipv4_bytes_to_string(&homa_message.destination_address)).unwrap(),
            255,
        );
        outgoing_datagram_transmitter_tx.send(packet).unwrap();

        for datagram in rx {
            println!("waiting on grant");
            match datagram.datagram_type {
                HomaDatagramType::Grant => {
                    if let Some(next_datagram) = datagrams.get(
                        datagram.sequence_number as usize / HOMA_DATAGRAM_PAYLOAD_LENGTH as usize,
                    ) {
                        let packet = next_datagram.to_ipv4(
                            Ipv4Addr::from_str(&ipv4_bytes_to_string(&homa_message.source_address))
                                .unwrap(),
                            Ipv4Addr::from_str(&ipv4_bytes_to_string(
                                &homa_message.destination_address,
                            ))
                            .unwrap(),
                            255,
                        );
                        outgoing_datagram_transmitter_tx.send(packet).unwrap();
                    } else {
                        return;
                    }
                }
                _ => {}
            }
        }
        println!("finished transmission")
    }
}
