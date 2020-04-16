//! Implementation of a SOCKS Protocol Version 5
//!
//! http://www.ietf.org/rfc/rfc1928.txt
//! http://en.wikipedia.org/wiki/SOCKS



mod server;

use server::Server;

fn main() {
    let _ = Server::new().run();
}