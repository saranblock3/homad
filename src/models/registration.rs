use bincode::deserialize;
use serde::Deserialize;
use serde::Serialize;
use std::io::Read;
use std::os::unix::net::UnixStream;

#[derive(Serialize, Deserialize, Debug)]
pub struct HomaRegistrationMessage {
    pub application_id: u32,
}

impl HomaRegistrationMessage {
    pub fn from_unix_stream(stream: &mut UnixStream) -> Result<Self, String> {
        let mut buffer = [0u8; 4];

        stream
            .read_exact(&mut buffer)
            .map_err(|_| "HomaRegisrationMessage not read")?;
        println!("registration size {:?}", buffer);

        deserialize(&buffer).map_err(|_| "HomaRegistrationMessage not deserialized".to_string())
    }
}
