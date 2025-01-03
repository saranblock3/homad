// use crate::HomaDatagramType::{Data, Resend};
// use bincode::{deserialize, serialize};
// use pnet::packet::ip::IpNextHeaderProtocol;
// use pnet::packet::Packet;
// use pnet::transport::TransportChannelType::Layer4;
// use pnet::transport::TransportProtocol::Ipv4;
// use pnet::transport::{ipv4_packet_iter, transport_channel, TransportReceiver, TransportSender};
// use serde::{Deserialize, Serialize};
// use serde_repr::{Deserialize_repr, Serialize_repr};
// use socket2::{Domain, Protocol, Socket, Type};
// use std::collections::HashMap;
// use std::fs::read;
// use std::io::{BufRead, BufReader, Read, Write};
// use std::net::{IpAddr, Ipv4Addr};
// use std::os::unix::net::{UnixListener, UnixStream};
// use std::path::Path;
// use std::sync::mpsc::{Receiver, Sender};
// use std::sync::{mpsc, Arc, Mutex};
// use std::{fs, thread};

// const HOMA_SOCKET_PATH: &str = "/tmp/homa.sock";

// #[derive(Serialize_repr, Deserialize_repr, Debug)]
// #[repr(u8)]
// enum HomaDatagramType {
//     Data,
//     Grant,
//     Resend,
//     Busy,
// }

// struct HomaMessage {
//     id: u64,
//     client_address: [u8; 4],
//     server_address: [u8; 4],
//     content: Vec<u8>,
// }

// #[derive(Serialize, Deserialize, Debug)]
// struct HomaDatagram {
//     datagram_type: HomaDatagramType,
//     message_id: u64,
//     source_address: [u8; 4],
//     destination_address: [u8; 4],
//     source_id: u64,
//     destination_id: u64,
//     sequence_number: u64,
//     flags: u8,
//     priority_control: u8,
//     message_length: u64,
//     datagram_length: u16,
//     // payload_length: u16,
//     payload: Vec<u8>,
//     checksum: [u8; 32],
// }

// #[derive(Serialize, Deserialize, Debug)]
// struct HomaInitialMessage {
//     application_id: u64,
// }

// struct Homa {
//     clients: Arc<Mutex<HashMap<u64, (Sender<HomaDatagram>, Sender<HomaDatagram>)>>>,
// }

// fn handle_incoming_datagram() {}

// impl Homa {
//     fn new() -> Self {
//         Homa {
//             clients: Arc::new(Mutex::new(HashMap::new())),
//         }
//     }

//     fn start(&mut self) {
//         // let read_socket = Socket::new_raw(Domain::IPV4, Type::from(146), Some(Protocol::from(146))).unwrap();
//         // let write_socket = read_socket.try_clone().unwrap();
//         let (write_socket_channel, read_socket_channel) =
//             transport_channel(4096, Layer4(Ipv4(IpNextHeaderProtocol(146)))).unwrap();

//         let (outgoing_datagram_tx, outgoing_datagram_rx): (
//             Sender<HomaDatagram>,
//             Receiver<HomaDatagram>,
//         ) = mpsc::channel();
//         thread::spawn(move || {
//             outgoing_datagram_handler(write_socket_channel, outgoing_datagram_rx);
//         });

//         let (priority_tx, priority_rx): (Sender<HomaDatagram>, Receiver<HomaDatagram>) =
//             mpsc::channel();
//         let outgoing_datagram_tx_clone = outgoing_datagram_tx.clone();
//         thread::spawn(move || {
//             priority_handler(priority_rx, outgoing_datagram_tx_clone);
//         });

//         let clients = Arc::clone(&self.clients);
//         thread::spawn(move || {
//             incoming_datagram_handler(read_socket_channel, clients);
//         });

//         let clients = Arc::clone(&self.clients);
//         // let outgoing_datagram_tx_clone = outgoing_datagram_tx.clone();
//         application_socket_handler(clients, priority_tx, outgoing_datagram_tx);
//     }
// }

// fn priority_handler(
//     priority_rx: Receiver<HomaDatagram>,
//     outgoing_datagram_tx: Sender<HomaDatagram>,
// ) {
// }
// fn incoming_datagram_handler(
//     mut read_socket_channel: TransportReceiver,
//     clients: Arc<Mutex<HashMap<u64, (Sender<HomaDatagram>, Sender<HomaDatagram>)>>>,
// ) {
//     let mut ipv4_packet_iter = ipv4_packet_iter(&mut read_socket_channel);

//     loop {
//         match ipv4_packet_iter.next() {
//             Ok((packet, addr)) => {
//                 if let Ok(homa_datagram) = deserialize::<HomaDatagram>(packet.payload()) {
//                     match homa_datagram.datagram_type {
//                         Data => {
//                             let clients = clients.lock().unwrap();
//                             if let Some(client) = clients.get(&homa_datagram.destination_id) {
//                                 client.0.send(homa_datagram).unwrap()
//                             }
//                         }
//                         _ => {
//                             let clients = clients.lock().unwrap();
//                             if let Some(client) = clients.get(&homa_datagram.destination_id) {
//                                 client.0.send(homa_datagram).unwrap()
//                             }
//                         }
//                     }
//                 }
//             }
//             Err(e) => {
//                 eprintln!("Error receiving packet: {}", e);
//             }
//         }
//     }
// }
// fn application_socket_handler(
//     clients: Arc<Mutex<HashMap<u64, (Sender<HomaDatagram>, Sender<HomaDatagram>)>>>,
//     priority_tx: Sender<HomaDatagram>,
//     outgoing_datagram_tx: Sender<HomaDatagram>,
// ) {
//     if Path::new(HOMA_SOCKET_PATH).exists() {
//         fs::remove_file(HOMA_SOCKET_PATH).unwrap();
//     }
//     let listener = UnixListener::bind(HOMA_SOCKET_PATH).unwrap();

//     for stream in listener.incoming() {
//         match stream {
//             Ok(mut stream) => {
//                 let initial_message = get_initial_message(&stream);
//                 let mut clients = clients.lock().unwrap();
//                 if clients.contains_key(&initial_message.application_id) {
//                     match stream.shutdown(std::net::Shutdown::Both) {
//                         Ok(()) => {}
//                         Err(err) => {
//                             eprintln!("application_socket_handler error: {:?}", err)
//                         }
//                     }
//                     continue;
//                 }
//                 stream.write_all(&[1, 2, 3, 4]).unwrap();
//                 println!("WROTE");
//                 let (incoming_handler_tx, incoming_handler_rx): (
//                     Sender<HomaDatagram>,
//                     Receiver<HomaDatagram>,
//                 ) = mpsc::channel();
//                 let (outgoing_handler_tx, outgoing_handler_rx): (
//                     Sender<HomaDatagram>,
//                     Receiver<HomaDatagram>,
//                 ) = mpsc::channel();
//                 clients.insert(
//                     initial_message.application_id,
//                     (incoming_handler_tx, outgoing_handler_tx),
//                 );

//                 let stream = Arc::new(Mutex::new(stream));
//                 let reader = Arc::clone(&stream);
//                 let writer = Arc::clone(&stream);

//                 thread::spawn(move || {
//                     outgoing_handler(reader, outgoing_handler_rx);
//                 });

//                 thread::spawn(move || {
//                     incoming_handler(writer, incoming_handler_rx);
//                 });

//                 println!("{:?}", initial_message);
//             }
//             Err(err) => eprintln!("accept function failed: {:?}", err),
//         }
//     }
// }

// fn get_initial_message(stream: &UnixStream) -> HomaInitialMessage {
//     let mut reader = BufReader::new(stream);
//     let mut buffer = [0u8; 8];

//     reader.read_exact(&mut buffer).unwrap();

//     let homa_initial_message: HomaInitialMessage = deserialize(&buffer).unwrap();

//     homa_initial_message
// }

// fn incoming_handler(writer: Arc<Mutex<UnixStream>>, incoming_handler_rx: Receiver<HomaDatagram>) {
//     loop {}
// }

// fn outgoing_handler(reader: Arc<Mutex<UnixStream>>, outgoing_handler_rx: Receiver<HomaDatagram>) {
//     let mut reader = reader.lock().unwrap();
//     loop {
//         let mut buffer = Vec::new();

//         if let Ok(n) = reader.read_to_end(&mut buffer) {}
//         thread::spawn(move || {
//             outgoing_message_handler(buffer);
//         });
//     }
// }

// fn outgoing_message_handler(content: Vec<u8>) {}

// fn outgoing_datagram_handler(
//     socket: TransportSender,
//     outgoing_datagram_rx: Receiver<HomaDatagram>,
// ) {
// }

// fn run_homa() {
//     let homa = Arc::new(Homa::new());
// }

// fn main() {
//     let datagram = HomaDatagram {
//         datagram_type: HomaDatagramType::Resend,
//         message_id: 1,
//         source_address: [1, 2, 3, 4],
//         destination_address: [1, 2, 3, 4],
//         source_id: 1,
//         destination_id: 1,
//         sequence_number: 1,
//         flags: 1,
//         priority_control: 1,
//         message_length: 1,
//         datagram_length: 1,
//         // payload_length: 1,
//         payload: vec![1, 2, 3, 4, 5, 6, 7, 8],
//         checksum: [1; 32],
//     };
//     let bytes = serialize(&datagram).unwrap();
//     println!("{:?}", datagram);
//     println!("{:?}", bytes);
//     Homa::new().start()
// }
