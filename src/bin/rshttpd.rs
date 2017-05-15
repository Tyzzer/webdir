#[macro_use] extern crate slog;
extern crate slog_term;
extern crate slog_async;
extern crate futures;
extern crate tokio_core;
extern crate hyper;
extern crate rshttpd;

use std::io;
use std::sync::Arc;
use std::net::SocketAddr;
use slog::{ Drain, Logger };
use slog_term::{ CompactFormat, TermDecorator };
use slog_async::Async;
use futures::Stream;
use tokio_core::reactor::Core;
use tokio_core::net::TcpListener;
use hyper::server::Http;
use rshttpd::Httpd;



#[inline]
fn start(addr: &SocketAddr) -> io::Result<()> {
    let decorator = TermDecorator::new().build();
    let drain = CompactFormat::new(decorator).build().fuse();
    let drain = Async::new(drain).build().fuse();
    let log = Logger::root(Arc::new(drain), o!("version" => env!("CARGO_PKG_VERSION")));

    let mut core = Core::new()?;
    let handle = core.handle();
    let httpd = Httpd::new(handle.clone(), log.clone())?;
    let listener = TcpListener::bind(addr, &handle)?;

    info!(log, "listening"; "addr" => format_args!("{}", listener.local_addr()?));

    let done = listener.incoming()
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
