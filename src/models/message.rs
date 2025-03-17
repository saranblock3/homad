use crate::config::CONFIG;
use crate::models::datagram;
use crate::models::datagram::HomaDatagram;
use crate::models::datagram::HomaDatagramBuilder;
use async_std::io::ReadExt;
use async_std::os::unix::net::UnixStream;
use bincode::deserialize;
use datagram::HomaDatagramType;
use derive_builder::Builder;
use serde::Deserialize;
use serde::Serialize;
use std::cmp::min;

#[derive(Serialize, Deserialize, Debug, Builder, Default)]
#[builder(default)]
pub struct HomaMessage {
    #[serde(skip)]
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

        if let Err(e) = stream.read_exact(&mut size_buffer).await {
            println!("size {:?}", size_buffer);
            return Err(e.to_string());
        }

        let size = u64::from_le_bytes(size_buffer);

        if size > CONFIG.MESSAGE_MAX_LENGTH {
            return Err("message too large".to_string());
        }

        let mut buffer = vec![0; size as usize];
        stream
            .read_exact(&mut buffer)
            .await
            .map_err(|_| "FROM UNIX STREAM FAILED MESSAGE")?;
        let result = deserialize(&buffer).ok();
        Ok(result)
    }

    pub fn split(&self) -> Vec<HomaDatagram> {
        let mut datagrams = Vec::<HomaDatagram>::new();
        let num_datagrams = (self.content.len() + (CONFIG.DATAGRAM_PAYLOAD_LENGTH as usize) - 1)
            / CONFIG.DATAGRAM_PAYLOAD_LENGTH as usize;
        for i in 0..num_datagrams {
            let datagram_length = min(
                self.content.len() - i * CONFIG.DATAGRAM_PAYLOAD_LENGTH as usize,
                CONFIG.DATAGRAM_PAYLOAD_LENGTH as usize,
            );
            let start = i * CONFIG.DATAGRAM_PAYLOAD_LENGTH as usize;
            let end = start + datagram_length;
            let datagram = HomaDatagramBuilder::default()
                .datagram_type(HomaDatagramType::Data)
                .message_id(self.id)
                .source_id(self.source_id)
                .destination_id(self.destination_id)
                .sequence_number(i as u32)
                .message_length(self.content.len() as u64)
                .payload(self.content[start..end].to_vec())
                .build()
                .unwrap();
            datagrams.push(datagram);
        }
        datagrams
    }
}
