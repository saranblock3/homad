mod actors;
mod constants;
mod models;
mod utils;

use crate::actors::application_listener::ApplicationListener;
use crate::actors::application_registrar::ApplicationRegistrarHandle;
use actors::workload_manager::*;
use actors::{
    application::ApplicationHandle, datagram_receiver::DatagramReceiver,
    datagram_sender::DatagramSenderHandle, priority_manager::PriorityManagerHandle,
};
use pnet::packet::ip::IpNextHeaderProtocol;
use pnet::transport::transport_channel;
use pnet::transport::TransportChannelType::Layer4;
use pnet::transport::TransportProtocol::Ipv4;
use rand::{random, thread_rng, Rng};
use std::ops::Range;
use std::{
    collections::HashMap,
    io,
    sync::{Arc, Mutex},
};

async fn start_homa() -> Result<(), io::Error> {
    let (transport_sender, transport_receiver) =
        transport_channel(3000000, Layer4(Ipv4(IpNextHeaderProtocol(146))))?;

    let workload_manager_handle = WorkloadManagerHandle::new();

    let priority_manager_handle = PriorityManagerHandle::new();

    let datagram_sender_handle = DatagramSenderHandle::new(transport_sender);

    let applications = Arc::new(Mutex::new(HashMap::<u32, ApplicationHandle>::new()));

    let applications_clone = Arc::clone(&applications);
    let application_registrar_handle = ApplicationRegistrarHandle::new(
        applications_clone,
        priority_manager_handle.clone(),
        workload_manager_handle.clone(),
        datagram_sender_handle,
    );

    ApplicationListener::start(application_registrar_handle);

    let applications_clone = Arc::clone(&applications);
    DatagramReceiver::start(transport_receiver, applications_clone);
    loop {}
}

#[tokio::main]
async fn main() {
    start_homa().await.unwrap();
}
