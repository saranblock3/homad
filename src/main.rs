mod actors;
mod constants;
mod homa;
mod models;
mod utils;

use actors::{
    application::ApplicationHandle, application_registrar::ApplicationRegistrar,
    datagram_receiver::DatagramReceiver, datagram_sender::DatagramSenderHandle,
};
use pnet::packet::ip::IpNextHeaderProtocol;
use pnet::transport::transport_channel;
use pnet::transport::TransportChannelType::Layer4;
use pnet::transport::TransportProtocol::Ipv4;
use std::{
    collections::HashMap,
    io,
    sync::{Arc, Mutex},
};

async fn start_homa() -> Result<(), io::Error> {
    let (transport_sender, transport_receiver) =
        transport_channel(4096, Layer4(Ipv4(IpNextHeaderProtocol(146))))?;

    let datagram_sender_handle = DatagramSenderHandle::new(transport_sender);

    let applications = Arc::new(Mutex::new(HashMap::<u32, ApplicationHandle>::new()));

    let applications_clone = Arc::clone(&applications);
    ApplicationRegistrar::start(applications_clone, datagram_sender_handle);

    let applications_clone = Arc::clone(&applications);
    DatagramReceiver::start(transport_receiver, applications_clone);
    loop {}
}

#[tokio::main]
async fn main() {
    start_homa().await.unwrap();
}
