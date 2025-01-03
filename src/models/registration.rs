use bincode::deserialize;
use serde::{Deserialize, Serialize};
use std::io::Read;
use std::os::unix::net::UnixStream;

#[derive(Serialize, Deserialize, Debug)]
pub struct HomaRegistrationMessage {
    pub application_id: u32,
}

impl HomaRegistrationMessage {
    pub fn from_unix_stream(stream: &mut UnixStream) -> Result<Self, bincode::Error> {
        let mut buffer = [0u8; 8];

        stream.read_exact(&mut buffer).unwrap();

        deserialize(&buffer)
    }
}
