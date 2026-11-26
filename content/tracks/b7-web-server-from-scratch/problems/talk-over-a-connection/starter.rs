use std::io;
use std::net::SocketAddr;

/// Listens on a free port of 127.0.0.1, serves on a background thread, and says where.
pub fn start(
    max_line: usize,
    handler: impl Fn(&str) -> String + Send + 'static,
) -> io::Result<SocketAddr> {
    todo!()
}
