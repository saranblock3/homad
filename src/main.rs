mod components;
mod config;
mod models;
mod utils;

use crate::components::application_listener::ApplicationListener;
use crate::components::application_registrar::ApplicationRegistrarHandle;
use components::application::ApplicationHandle;
use components::datagram_receiver::DatagramReceiver;
use components::datagram_sender::DatagramSenderHandle;
use components::priority_manager::PriorityManagerHandle;
use components::workload_manager::*;
use pnet::packet::ip::IpNextHeaderProtocol;
use pnet::transport::transport_channel;
use pnet::transport::TransportChannelType::Layer4;
use pnet::transport::TransportProtocol::Ipv4;
use std::collections::HashMap;
use std::io;
use std::sync::Arc;
use std::sync::Mutex;

fn start_homa() -> Result<(), io::Error> {
    let (transport_sender, transport_receiver) =
        transport_channel(3000000, Layer4(Ipv4(IpNextHeaderProtocol(146))))?;

    let workload_manager_handle = WorkloadManagerHandle::new();

    let priority_manager_handle = PriorityManagerHandle::new();

    let datagram_sender_handle = DatagramSenderHandle::new(transport_sender);

    let application_handles = Arc::new(Mutex::new(HashMap::<u32, ApplicationHandle>::new()));

    let application_handles_clone = Arc::clone(&application_handles);
    let application_registrar_handle = ApplicationRegistrarHandle::new(
        application_handles_clone,
        priority_manager_handle.clone(),
        workload_manager_handle.clone(),
        datagram_sender_handle,
    );

    ApplicationListener::start(application_registrar_handle).unwrap();

    let application_handles_clone = Arc::clone(&application_handles);
    DatagramReceiver::start_many(transport_receiver, application_handles_clone);
    loop {}
}

#[tokio::main]
async fn main() {
    start_homa().unwrap();
}
