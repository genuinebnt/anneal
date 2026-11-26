use std::io;
use std::net::SocketAddr;

pub struct Server {
    // TODO
}

impl Server {
    pub fn start(answer: impl Fn(&str) -> String + Send + Sync + 'static) -> io::Result<Server> {
        todo!()
    }

    pub fn addr(&self) -> SocketAddr {
        todo!()
    }

    /// Shuts down and returns the number of client connections accepted.
    pub fn stop(self) -> usize {
        todo!()
    }
}
