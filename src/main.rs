mod constants;
mod homa;
mod models;
mod utils;

use homa::core::Homa;

fn main() {
    Homa::new().start().unwrap();
}
