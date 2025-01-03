// use std::collections::HashMap;
// use std::os::unix::net::{UnixListener, UnixStream};
// use std::path::Path;
// use std::fs;
// use std::io::{BufRead, BufReader, Read};
// use bincode::{serialize, deserialize};
// use serde::{Serialize, Deserialize};
// use std::thread;
// use std::sync::{mpsc, Mutex};
// use std::sync::mpsc::{Receiver, Sender};
// use rand::{random, Rng};
// use serde_json::from_slice;

// const HOMA_SOCKET_PATH: &str = "/tmp/homa.sock";

// #[derive(Serialize, Deserialize, Debug)]
// struct HomaDatagram {
//     datagram_type: u8,
//     message_id: u64,
//     source: [u8; 4],
//     destination: [u8; 4],
//     sequence_number: u64,
//     flags: u8,
//     priority_control: u8,
//     message_length: u64,
//     datagram_length: u16,
//     payload_length: u16,
//     payload: Vec<u8>,
//     checksum: [u8; 32]
// }

// #[derive(Serialize, Deserialize, Debug)]
// struct HomaMessage {
//     id: u64,
//     client_address: [u8; 4],
//     server_address: [u8; 4],
//     content: Vec<u8>,
// }

// #[derive(Serialize, Deserialize, Debug)]
// struct HomaInitialMessage {
//     id: u64,
//     client_address: [u8; 4],
//     server_address: [u8; 4]
// }

// struct Homa {
//     messages: Mutex<HashMap<u64, (Sender<HomaDatagram>, Sender<HomaDatagram>)>,
// }

// impl Homa {
//     fn new() -> Self {
//         Homa {
//             messages: Mutex::new(HashMap::new()),
//         }
//     }

//     fn accept_client(&self, unix_stream: UnixStream) {
//         let homa_initial_message = get_initial_message(unix_stream);

//         let (incoming_tx, incoming_rx) = mpsc::channel();
//         let (outgoing_tx, outgoing_rx) = mpsc::channel();
//         let mut messages = self.messages.lock().unwrap();
//         if messages.contains_key(&homa_initial_message.id) {
//             return
//         }
//         messages.insert(homa_initial_message.id, (incoming_tx, outgoing_tx));
//     }
// }

// fn get_initial_message(unix_stream: UnixStream) -> HomaInitialMessage {
//     let mut buffer = Vec::new();
//     let mut reader = BufReader::new(unix_stream);
//     let mut temp_buffer = [0u8; 1024];

//     loop {
//         let n = reader.read(&mut temp_buffer).unwrap();

//         if n == 0 {
//             break;
//         }

//         buffer.extend_from_slice(&temp_buffer[..n]);
//     }

//     let homa_initial_message: HomaInitialMessage = deserialize(buffer.as_slice()).unwrap();

//     homa_initial_message

// }

// fn main() -> std::io::Result<()> {
//     let datagram = HomaDatagram{
//         datagram_type: 1,
//         message_id: 1,
//         source: [1, 2, 3, 4],
//         destination: [1, 2, 3, 4],
//         sequence_number: 1,
//         flags: 1,
//         priority_control: 1,
//         message_length: 1,
//         datagram_length: 1,
//         payload_length: 1,
//         payload: vec![1, 2, 3, 4, 5, 6, 7, 8],
//         checksum: [1; 32]

//     };
//     let bytes = serialize(&datagram).unwrap();
//     println!("{:?}", datagram);
//     println!("{:?}", bytes);
//     let other: HomaDatagram = deserialize(&bytes).unwrap();
//     println!("{:?}", other);
//     if Path::new(HOMA_SOCKET_PATH).exists() {
//         fs::remove_file(HOMA_SOCKET_PATH)?;
//     }
//     let listener = UnixListener::bind(HOMA_SOCKET_PATH)?;

//     for stream in listener.incoming() {
//         match stream {
//             Ok(stream) => {
//                 // println!("{:?}", stream);
//                 // stream.write_all(b"hello world")?;
//                 // let mut response = String::new();
//                 // let _ = stream.read_to_string(&mut response);
//                 // println!("{}", response)
//                 client_initiator(stream);
//             },
//             Err(err) => eprintln!("accept function failed: {:?}", err)
//         }
//     }
//     Ok(())
// }
