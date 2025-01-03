mod constants;
mod homa;
mod models;
mod utils;

use homa::core::Homa;
use std::sync::{mpsc, Arc};
use std::thread;

fn start_homa() {
    // let (outgoing_datagram_transmitter_tx, outgoing_datagram_transmitter_rx) = mpsc::channel();
    let homa = Arc::new(Homa::new().unwrap());

    {
        let homa = Arc::clone(&homa);
        thread::spawn(move || {
            homa.incoming_datagram_listener();
        });
    }

    {
        let homa = Arc::clone(&homa);
        thread::spawn(move || {
            homa.outgoing_datagram_transmitter();
        });
    }

    homa.application_listener();
}

fn main() {
    start_homa();
}
