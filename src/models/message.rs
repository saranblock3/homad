use async_std::io::ReadExt;
use async_std::os::unix::net::UnixStream;
use bincode::deserialize;
use datagram::HomaDatagramType;
use derive_builder::Builder;
use serde::{Deserialize, Serialize};
use std::os::fd::{AsRawFd, FromRawFd};
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
    pub async fn from_unix_stream(stream: &mut UnixStream) -> Result<Option<Self>, String> {
        let mut size_buffer = [0u8; 8];

        if let Err(_) = stream.read_exact(&mut size_buffer).await {
            println!("FROM UNIX STREAM FAILED SIZE");
            return Err("stream not read".to_string());
        }

        let size = u64::from_le_bytes(size_buffer);

        if size > HOMA_MESSAGE_MAX_LENGTH {
            println!("FROM UNIX STREAM FAILED CAPACITY");
            return Err("message too large".to_string());
        }

        let mut buffer = vec![0; size as usize];
        stream
            .read_exact(&mut buffer)
            .await
            .map_err(|_| "FROM UNIX STREAM FAILED MESSAGE")?;
        let result = deserialize(&buffer).ok();
        if let None = result {
            println!("FROM UNIX STREAM FAILED MESSAGE");
        }
        Ok(result)
    }

    pub fn split(&self) -> Vec<HomaDatagram> {
        let mut datagrams = Vec::<HomaDatagram>::new();
        let num_datagrams = (self.content.len() + (HOMA_DATAGRAM_PAYLOAD_LENGTH as usize) - 1)
            / HOMA_DATAGRAM_PAYLOAD_LENGTH as usize;
        for i in 0..num_datagrams {
            let datagram_length = min(
                self.content.len() - i * HOMA_DATAGRAM_PAYLOAD_LENGTH as usize,
                HOMA_DATAGRAM_PAYLOAD_LENGTH as usize,
            );
            let start = i * HOMA_DATAGRAM_PAYLOAD_LENGTH as usize;
            let end = start + datagram_length;
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
                .payload(self.content[start..end].to_vec())
                .build()
                .unwrap();
            datagrams.push(datagram);
        }
        datagrams
    }
}
