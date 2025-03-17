use crate::config::CONST;
use bincode::serialize;
use crc32fast::Hasher;
use derive_builder::Builder;
use pnet::packet::ipv4::MutableIpv4Packet;
use serde::Deserialize;
use serde::Serialize;
use serde_repr::Deserialize_repr;
use serde_repr::Serialize_repr;
use std::net::Ipv4Addr;
use std::u64;

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
    pub source_id: u32,
    pub destination_id: u32,
    pub sequence_number: u32,
    pub workload: [u64; CONST::UNSCHEDULED_PRIORITY_PARTITIONS],
    pub priority: u8,
    pub message_length: u64,
    pub payload: Vec<u8>,
    pub checksum: u32,
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

    pub fn checksum(&mut self) -> Result<u32, String> {
        self.checksum = 0;
        let datagram_bytes = serialize(self).map_err(|_| "Failed to checksum datagram")?;
        let mut hasher = Hasher::new();
        hasher.update(&datagram_bytes);
        let checksum = hasher.finalize();
        self.checksum = checksum;
        Ok(checksum)
    }
}

#[cfg(test)]
mod tests {
    use super::HomaDatagram;

    #[test]
    fn checksum_test() {
        let mut datagram = HomaDatagram::default();
        let checksum = datagram.checksum();
        assert!(checksum == datagram.checksum());
    }
}
