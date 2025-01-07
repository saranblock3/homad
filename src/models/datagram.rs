use std::{net::Ipv4Addr, u64};

use bincode::serialize;
use derive_builder::Builder;
use pnet::packet::ipv4::{Ipv4, Ipv4Packet, MutableIpv4Packet};
use serde::{Deserialize, Serialize};
use serde_repr::{Deserialize_repr, Serialize_repr};
use std::str::FromStr;

use crate::{constants::HOMA_DATAGRAM_PAYLOAD_LENGTH, utils::ipv4_bytes_to_string};

use super::message::{HomaMessage, HomaMessageBuilder};

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

    pub fn grant(&self, priority: u8) -> Self {
        let mut grant = self.clone();
        grant.datagram_type = HomaDatagramType::Grant;
        grant.sequence_number += self.payload.len() as u64;
        grant.swap_endpoints();
        grant.priority_control = priority;
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

#[derive(Builder)]
pub struct HomaDatagramCollector {
    message_id: u64,
    source_address: [u8; 4],
    destination_address: [u8; 4],
    source_id: u32,
    destination_id: u32,
    expected: u64,
    collected: u64,
    content: Vec<u8>,
}

impl HomaDatagramCollector {
    pub fn new(datagram: &HomaDatagram) -> Self {
        HomaDatagramCollectorBuilder::default()
            .message_id(datagram.message_id)
            .source_address(datagram.source_address)
            .destination_address(datagram.destination_address)
            .source_id(datagram.source_id)
            .destination_id(datagram.destination_id)
            .expected(datagram.message_length)
            .collected(datagram.payload.len() as u64)
            .content(datagram.payload.clone())
            .build()
            .unwrap()
    }

    pub fn add(&mut self, datagram: &HomaDatagram) {
        if self.collected > datagram.sequence_number {
            return;
        }
        self.collected += datagram.payload.len() as u64;
        self.content.append(&mut datagram.payload.clone());
    }

    pub fn priority(&self) -> u8 {
        let remainder = self.expected - self.collected;
        if remainder / HOMA_DATAGRAM_PAYLOAD_LENGTH as u64 > 255 {
            0
        } else {
            (255 - remainder / HOMA_DATAGRAM_PAYLOAD_LENGTH as u64) as u8
        }
    }

    pub fn is_complete(&self) -> bool {
        self.collected >= self.expected
    }

    pub fn to_message(self) -> HomaMessage {
        HomaMessageBuilder::default()
            .id(self.message_id)
            .source_address(self.source_address)
            .destination_address(self.destination_address)
            .source_id(self.source_id)
            .destination_id(self.destination_id)
            .content(self.content)
            .build()
            .unwrap()
    }
}
