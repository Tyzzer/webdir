#[macro_use] extern crate slog;
extern crate slog_term;
extern crate slog_async;
#[macro_use] extern crate structopt_derive;
extern crate structopt;
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
use structopt::StructOpt;
use futures::Stream;
use tokio_core::reactor::Core;
use tokio_core::net::TcpListener;
use hyper::server::Http;
use rshttpd::Httpd;


#[derive(StructOpt)]
#[structopt(name = "rshttpd", about = "A simple static webserver")]
struct Config {
    #[structopt(long = "bind", help = "specify bind address", default_value = "127.0.0.1:0")]
    addr: SocketAddr,
    #[structopt(long = "root", help = "specify root path")]
    root: Option<String>,
}


#[inline]
fn start(config: Config) -> io::Result<()> {
    let decorator = TermDecorator::new().build();
    let drain = CompactFormat::new(decorator).build().fuse();
    let drain = Async::new(drain).build().fuse();
    let log = Logger::root(Arc::new(drain), o!("version" => env!("CARGO_PKG_VERSION")));

    let mut core = Core::new()?;
    let handle = core.handle();
    let listener = TcpListener::bind(&config.addr, &handle)?;
    let mut httpd = Httpd::new(handle.clone(), log.clone())?;

    if let Some(p) = config.root {
        httpd.set_root(p)?;
    }

    info!(log, "listening";
        "addr" => format_args!("{}", listener.local_addr()?),
        "root" => format_args!("{}", httpd.root.display())
    );

    let done = listener.incoming()
        .for_each(|(stream, addr)| {
            Http::new().bind_connection(
                &handle, stream, addr,
                httpd.clone()
            );

            Ok(())
        });

    core.run(done)
}


fn main() {
    start(Config::from_args()).unwrap();
}
