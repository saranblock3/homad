use bincode::deserialize;
use datagram::HomaDatagramType;
use serde::{Deserialize, Serialize};
use std::iter::StepBy;
use std::os::unix::net::UnixStream;
use std::sync::Arc;
use std::{cmp::min, io::Read};

use crate::constants::HOMA_DATAGRAM_PAYLOAD_LENGTH;

use super::datagram::{self, HomaDatagram};

#[derive(Serialize, Deserialize, Debug)]
pub struct HomaMessage {
    pub id: u64,
    pub source_address: [u8; 4],
    pub destination_address: [u8; 4],
    pub source_id: u32,
    pub destination_id: u32,
    content: Vec<u8>,
}

impl HomaMessage {
    pub fn from_unix_stream(stream: &mut UnixStream) -> Result<Self, bincode::Error> {
        let mut size_buffer = [0u8; 8];

        stream.read_exact(&mut size_buffer).unwrap();

        let size = u64::from_le_bytes(size_buffer);
        println!("size: {}", size);

        let mut buffer = vec![0; size as usize];
        stream.read_exact(&mut buffer).unwrap();
        return deserialize(&buffer);
    }

    pub fn split(&self) -> Vec<HomaDatagram> {
        let mut datagrams = Vec::<HomaDatagram>::new();
        for i in (0..self.content.len()).step_by(HOMA_DATAGRAM_PAYLOAD_LENGTH as usize) {
            let datagram_length = min(
                self.content.len() - i,
                HOMA_DATAGRAM_PAYLOAD_LENGTH as usize,
            );
            let datagram = HomaDatagram {
                datagram_type: HomaDatagramType::Data,
                message_id: self.id,
                source_address: self.source_address,
                destination_address: self.destination_address,
                source_id: self.source_id,
                destination_id: self.destination_id,
                sequence_number: i as u64,
                flags: 0,
                priority_control: 0,
                message_length: self.content.len() as u64,
                datagram_length: datagram_length as u16,
                payload: self.content[i..i + datagram_length].to_vec(),
                checksum: [0; 4],
            };
            datagrams.push(datagram);
        }
        datagrams
    }

    // pub fn to_homa_datagrams(&self) -> Option<Vec<HomaDatagram>> {
    //     let homa_datagrams = Vec::new();
    //     let message_id = random::<u64>();
    //     for i in (0..self.content.len()).step_by(HOMA_DATAGRAM_PAYLOAD_LENGTH as usize) {
    //         let homa_datagram = HomaDatagram{
    //             datagram_type: HomaDatagramType::Data,
    //             message_id: message_id,
    //             source_address: self.client_address,
    //             destination_address: self.server_address,
    //         }
    //     }
    //     None
    // }
}
