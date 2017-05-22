#[macro_use] extern crate slog;
extern crate slog_term;
extern crate slog_async;
#[macro_use] extern crate structopt_derive;
extern crate structopt;
extern crate futures;
extern crate tokio_core;
extern crate hyper;
extern crate rustls;
extern crate tokio_rustls;
#[macro_use] extern crate rshttpd;

use std::fs::File;
use std::sync::Arc;
use std::net::SocketAddr;
use std::io::{ self, BufReader };
use slog::{ Drain, Logger };
use slog_term::{ CompactFormat, TermDecorator };
use slog_async::Async;
use structopt::StructOpt;
use futures::{ Future, Stream };
use tokio_core::reactor::Core;
use tokio_core::net::TcpListener;
use hyper::server::Http;
use rustls::{ Certificate, PrivateKey, ServerConfig, ServerSessionMemoryCache };
use rustls::internal::pemfile::{ certs, rsa_private_keys };
use tokio_rustls::ServerConfigExt;
use rshttpd::Httpd;


#[derive(StructOpt)]
#[structopt(name = "rshttpd", about = "A simple static webserver")]
struct Config {
    #[structopt(long = "bind", help = "specify bind address", default_value = "127.0.0.1:0")]
    addr: SocketAddr,
    #[structopt(long = "root", help = "specify root path")]
    root: Option<String>,
    #[structopt(long = "cert", help = "specify TLS cert path", requires = "key")]
    cert: Option<String>,
    #[structopt(long = "key", help = "specify TLS key path", requires = "cert")]
    key: Option<String>,
    #[structopt(long = "session-buff", help = "specify TLS session buff size", default_value = "64")]
    session_buff: usize
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

    let opt_tls_config = if let (Some(ref cert), Some(ref key)) = (config.cert, config.key) {
        let mut tls_config = ServerConfig::new();
        tls_config.set_single_cert(load_certs(&cert)?, load_keys(&key)?.remove(0));
        tls_config.set_persistence(ServerSessionMemoryCache::new(config.session_buff));
        Some(Arc::new(tls_config))
    } else {
        None
    };

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
            if let Some(ref tls_config) = opt_tls_config {
                let handle2 = handle.clone();
                let log = log.clone();
                let httpd = httpd.clone();

                let done = tls_config.accept_async(stream)
                    .map(move |stream| Http::new().bind_connection(&handle2, stream, addr, httpd))
                    .map_err(move |err| error!(log, "tls"; "error" => format_args!("{}", err)));
                handle.spawn(done);
            } else {
                Http::new().bind_connection(&handle, stream, addr, httpd.clone());
            }

            Ok(())
        });

    core.run(done)
}


fn main() {
    start(Config::from_args()).unwrap();
}

fn load_certs(path: &str) -> io::Result<Vec<Certificate>> {
    certs(&mut BufReader::new(File::open(path)?))
        .map_err(|_| err!(Other, "Not found cert"))
}

fn load_keys(path: &str) -> io::Result<Vec<PrivateKey>> {
    rsa_private_keys(&mut BufReader::new(File::open(path)?))
        .map_err(|_| err!(Other, "Not found keys"))
}
