use bincode::deserialize;
use datagram::HomaDatagramType;
use derive_builder::Builder;
use serde::{Deserialize, Serialize};
use std::io::Error;
use std::iter::StepBy;
use std::os::unix::net::UnixStream;
use std::sync::Arc;
use std::{cmp::min, io::Read};

use crate::constants::{HOMA_DATAGRAM_PAYLOAD_LENGTH, HOMA_MESSAGE_MAX_LENGTH};

use super::datagram::{self, HomaDatagram, HomaDatagramBuilder};

#[derive(Serialize, Deserialize, Debug, Builder, Default)]
#[builder(default)]
pub struct HomaMessage {
    pub id: u64,
    pub source_address: [u8; 4],
    pub destination_address: [u8; 4],
    pub source_id: u32,
    pub destination_id: u32,
    pub content: Vec<u8>,
}

impl HomaMessage {
    pub fn from_unix_stream(stream: &mut UnixStream) -> Option<Self> {
        let mut size_buffer = [0u8; 8];

        if let Err(_) = stream.read_exact(&mut size_buffer) {
            println!("stream not read");
            return None;
        }

        let size = u64::from_le_bytes(size_buffer);

        if size > HOMA_MESSAGE_MAX_LENGTH {
            return None;
        }

        let mut buffer = vec![0; size as usize];
        stream.read_exact(&mut buffer).unwrap();
        return deserialize(&buffer).ok();
    }

    pub fn split(&self) -> Vec<HomaDatagram> {
        let mut datagrams = Vec::<HomaDatagram>::new();
        for i in (0..self.content.len()).step_by(HOMA_DATAGRAM_PAYLOAD_LENGTH as usize) {
            let datagram_length = min(
                self.content.len() - i,
                HOMA_DATAGRAM_PAYLOAD_LENGTH as usize,
            );
            let datagram = HomaDatagramBuilder::default()
                .datagram_type(HomaDatagramType::Data)
                .message_id(self.id)
                .source_address(self.source_address)
                .destination_address(self.destination_address)
                .source_id(self.source_id)
                .destination_id(self.destination_id)
                .sequence_number(i as u64)
                .message_length(self.content.len() as u64)
                .datagram_length(datagram_length as u16)
                .payload(self.content[i..i + datagram_length].to_vec())
                .build()
                .unwrap();
            datagrams.push(datagram);
        }
        datagrams
    }

    pub fn from_datagram(datagram: &HomaDatagram) -> Self {
        HomaMessageBuilder::default()
            .id(datagram.message_id)
            .source_address(datagram.source_address)
            .destination_address(datagram.destination_address)
            .source_id(datagram.source_id)
            .destination_id(datagram.destination_id)
            .content(datagram.payload.clone())
            .build()
            .unwrap()
    }

    pub fn add_datagram(&mut self, datagram: &HomaDatagram) {}

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
