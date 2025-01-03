use std::net::Ipv4Addr;

use bincode::serialize;
use derive_builder::Builder;
use pnet::packet::ipv4::{Ipv4, Ipv4Packet, MutableIpv4Packet};
use serde::{Deserialize, Serialize};
use serde_repr::{Deserialize_repr, Serialize_repr};

use crate::constants::HOMA_DATAGRAM_PAYLOAD_LENGTH;

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
    pub flags: u8,
    pub priority_control: u8,
    pub message_length: u64,
    pub datagram_length: u16,
    pub payload: Vec<u8>,
    pub checksum: [u8; 4],
}

impl HomaDatagram {
    pub fn to_ipv4(
        &self,
        source_address: Ipv4Addr,
        destination_address: Ipv4Addr,
        priority: u8,
    ) -> Vec<u8> {
        let payload = serialize::<HomaDatagram>(self).unwrap();
        let mut buffer = vec![0u8; 20 + payload.len()];
        let mut packet = MutableIpv4Packet::new(&mut buffer).unwrap();
        packet.set_version(4);
        packet.set_header_length(5);
        packet.set_total_length(20 + payload.len() as u16);
        packet.set_ttl(64);
        packet.set_dscp(priority);
        packet.set_source(source_address);
        packet.set_destination(destination_address);
        buffer[20..].copy_from_slice(&payload);
        buffer
    }

    pub fn grant(&self) -> Self {
        let mut grant = self.clone();
        grant.datagram_type = HomaDatagramType::Grant;
        grant.sequence_number += HOMA_DATAGRAM_PAYLOAD_LENGTH as u64;
        grant.swap_endpoints();
        grant.payload = Vec::new();
        grant
    }

    fn swap_endpoints(&mut self) {
        let tmp_address = self.source_address;
        self.source_address = self.destination_address;
        self.destination_address = tmp_address;

        let tmp_id = self.source_id;
        self.source_id = self.destination_id;
        self.destination_id = tmp_id;
    }
}
