#[macro_use] extern crate slog;
extern crate slog_term;
extern crate slog_async;
#[macro_use] extern crate structopt_derive;
extern crate structopt;
extern crate tokio_proto;
extern crate hyper;
extern crate rustls;
extern crate tokio_rustls;
#[macro_use] extern crate rshttpd;

use std::env;
use std::fs::File;
use std::sync::Arc;
use std::net::SocketAddr;
use std::path::Path;
use std::io::{ self, BufReader };
use slog::{ Drain, Logger };
use slog_term::{ CompactFormat, TermDecorator };
use slog_async::Async;
use structopt::StructOpt;
use tokio_proto::TcpServer;
use hyper::server::Http;
use rustls::{ Certificate, PrivateKey, ServerConfig, ServerSessionMemoryCache };
use rustls::internal::pemfile::{ certs, rsa_private_keys };
use tokio_rustls::proto::Server;
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
    session_buff: usize,
    #[structopt(long = "threads", help = "specify threads", default_value = "1")]
    threads: usize
}


#[inline]
fn start(config: Config) -> io::Result<()> {
    let decorator = TermDecorator::new().build();
    let drain = CompactFormat::new(decorator).build().fuse();
    let drain = Async::new(drain).build().fuse();
    let log = Logger::root(Arc::new(drain), o!("version" => env!("CARGO_PKG_VERSION")));

    let opt_tls_config = if let (Some(ref cert), Some(ref key)) = (config.cert, config.key) {
        let mut tls_config = ServerConfig::new();
        tls_config.set_single_cert(load_certs(&cert)?, load_keys(&key)?.remove(0));
        tls_config.set_persistence(ServerSessionMemoryCache::new(config.session_buff));
        Some(Arc::new(tls_config))
    } else {
        None
    };

    let root = if let Some(ref p) = config.root {
        Arc::new(Path::new(p).canonicalize()?)
    } else {
        Arc::new(env::current_dir()?)
    };

    info!(log, "listening";
        "addr" => format_args!("{}", config.addr), // FIXME socket addr
        "root" => format_args!("{}", root.display())
    );

    if let Some(tls_config) = opt_tls_config {
        let mut server = TcpServer::new(Server::new(Http::new(), tls_config), config.addr);
        server.threads(config.threads);
        server.with_handle(move |handle| {
            let httpd = Httpd::new(handle.remote().clone(), log.clone(), root.clone());
            move || Ok(httpd.clone())
        });
    } else {
        let mut server = TcpServer::new(Http::new(), config.addr);
        server.threads(config.threads);
        server.with_handle(move |handle| {
            let httpd = Httpd::new(handle.remote().clone(), log.clone(), root.clone());
            move || Ok(httpd.clone())
        });
    }

    Ok(())
}


fn main() {
    start(Config::from_args()).unwrap();
}

#[inline]
fn load_certs(path: &str) -> io::Result<Vec<Certificate>> {
    certs(&mut BufReader::new(File::open(path)?))
        .map_err(|_| err!(Other, "Not found cert"))
}

#[inline]
fn load_keys(path: &str) -> io::Result<Vec<PrivateKey>> {
    rsa_private_keys(&mut BufReader::new(File::open(path)?))
        .map_err(|_| err!(Other, "Not found keys"))
}
