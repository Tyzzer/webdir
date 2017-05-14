extern crate futures;
extern crate tokio_core;
extern crate hyper;
extern crate rhttpd;

use std::io;
use std::net::SocketAddr;
use futures::Stream;
use tokio_core::reactor::Core;
use tokio_core::net::TcpListener;
use hyper::server::Http;
use rhttpd::Httpd;



#[inline]
fn start(addr: &SocketAddr) -> io::Result<()> {
    let mut core = Core::new()?;
    let handle = core.handle();
    let httpd = Httpd::new(&handle)?;

    let done = TcpListener::bind(addr, &handle)?
        .incoming()
        .for_each(|(stream, addr)| {
            Http::new().bind_connection(
                &handle, stream, addr,
                httpd.clone()
            );

            Ok(())
        });

    core.run(done)?;

    Ok(())
}


fn main() {
    let addr = "127.0.0.1:1337".parse().unwrap();
    start(&addr).unwrap();
}
