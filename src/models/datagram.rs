use std::{net::Ipv4Addr, u64};

use bincode::serialize;
use derive_builder::Builder;
use pnet::packet::ipv4::MutableIpv4Packet;
use serde::{Deserialize, Serialize};
use serde_repr::{Deserialize_repr, Serialize_repr};
use std::str::FromStr;

use crate::utils::ipv4_bytes_to_string;

#[derive(Serialize_repr, Deserialize_repr, Debug, Default, Clone)]
#[repr(u8)]
pub enum HomaDatagramType {
    #[default]
    Data,
    Grant,
    Resend,
    Busy,
}

#[derive(Serialize, Deserialize, Debug, Builder, Default, Clone)]
#[builder(default)]
pub struct HomaDatagram {
    pub datagram_type: HomaDatagramType,
    pub message_id: u64,
    pub source_address: [u8; 4],
    pub destination_address: [u8; 4],
    pub source_id: u32,
    pub destination_id: u32,
    pub sequence_number: u64,
    pub workload: Vec<u64>,
    pub priority: u8,
    pub message_length: u64,
    pub datagram_length: u16,
    pub payload: Vec<u8>,
    pub checksum: [u8; 4],
}

impl HomaDatagram {
    pub fn to_ipv4(&self, priority: u8) -> Vec<u8> {
        let payload = serialize::<HomaDatagram>(self).unwrap();
        let mut buffer = vec![0u8; 20 + payload.len()];
        let mut packet = MutableIpv4Packet::new(&mut buffer).unwrap();
        packet.set_version(4);
        packet.set_header_length(5);
        packet.set_total_length(20 + payload.len() as u16);
        packet.set_ttl(64);
        packet.set_dscp(priority);
        packet.set_source(Ipv4Addr::from_str(&ipv4_bytes_to_string(&self.source_address)).unwrap());
        packet.set_destination(
            Ipv4Addr::from_str(&ipv4_bytes_to_string(&self.destination_address)).unwrap(),
        );
        buffer[20..].copy_from_slice(&payload);
        buffer
    }
}
