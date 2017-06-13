#[macro_use] extern crate slog;
extern crate slog_term;
extern crate slog_async;
#[macro_use] extern crate structopt_derive;
extern crate structopt;
#[macro_use] extern crate serde_derive;
extern crate serde;
extern crate toml;
extern crate xdg;
extern crate futures;
extern crate hyper;
extern crate tokio_core;
extern crate rustls;
extern crate tokio_rustls;
#[macro_use] extern crate webdir;

use std::env;
use std::fs::File;
use std::sync::Arc;
use std::net::{ SocketAddr, IpAddr, Ipv4Addr };
use std::path::{ Path, PathBuf };
use std::io::{ self, Read, BufReader };
use slog::{ Drain, Logger };
use slog_term::{ CompactFormat, TermDecorator };
use slog_async::Async;
use structopt::StructOpt;
use futures::{ Future, Stream };
use hyper::server::Http;
use tokio_core::reactor::Core;
use tokio_core::net::TcpListener;
use rustls::{ Certificate, PrivateKey, ServerConfig, ServerSessionMemoryCache };
use rustls::internal::pemfile::{ certs, rsa_private_keys };
use tokio_rustls::ServerConfigExt;
use webdir::Httpd;


#[derive(StructOpt)]
#[derive(Serialize, Deserialize)]
#[structopt]
struct Config {
    #[structopt(long = "bind", help = "bind address")]
    addr: Option<SocketAddr>,
    #[structopt(long = "root", help = "root path")]
    root: Option<String>,
    #[structopt(long = "cert", help = "TLS cert", requires = "key")]
    cert: Option<String>,
    #[structopt(long = "key", help = "TLS key", requires = "cert")]
    key: Option<String>,
    #[structopt(long = "session-buff", help = "TLS session buff")]
    session_buff: Option<usize>,

    #[serde(skip_serializing)]
    #[structopt(short = "c", long = "config", help = "Read config from File")]
    config: Option<String>
}


#[inline]
fn start(config: Config) -> io::Result<()> {
    let decorator = TermDecorator::new().build();
    let drain = CompactFormat::new(decorator).build().fuse();
    let drain = Async::new(drain).build().fuse();
    let log = Logger::root(Arc::new(drain), o!("version" => env!("CARGO_PKG_VERSION")));

    let maybe_tls_config = if let (Some(ref cert), Some(ref key)) = (config.cert, config.key) {
        let mut tls_config = ServerConfig::new();
        tls_config.set_single_cert(load_certs(cert)?, load_keys(key)?.remove(0));
        tls_config.set_persistence(ServerSessionMemoryCache::new(config.session_buff.unwrap_or(64)));
        Some(Arc::new(tls_config))
    } else {
        None
    };

    let root = if let Some(ref p) = config.root {
        Arc::new(Path::new(p).canonicalize()?)
    } else {
        Arc::new(env::current_dir()?)
    };

    let mut core = Core::new()?;
    let handle = core.handle();
    let remote = handle.remote().clone();
    let listener = TcpListener::bind(
        &config.addr.unwrap_or_else(|| SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 0)),
        &handle
    )?;

    info!(log, "listening";
        "root" => format_args!("{}", root.display()),
        "listen" => format_args!("{}", listener.local_addr()?)
    );

    let done = listener.incoming().for_each(|(stream, addr)| {
        let log = log.new(o!("addr" => format!("{}", addr)));
        let httpd = Httpd::new(remote.clone(), log.clone(), root.clone());

        if let Some(ref tls_config) = maybe_tls_config {
            let handle2 = handle.clone();

            let done = tls_config.accept_async(stream)
                .map(move |stream| Http::new()
                    .keep_alive(true)
                    .bind_connection(&handle2, stream, addr, httpd)
                )
                .map_err(move |err| error!(log, "tls"; "err" => format_args!("{}", err)));

            handle.spawn(done);
        } else {
            Http::new().bind_connection(&handle, stream, addr, httpd);
        }

        Ok(())
    });

    core.run(done)
}


fn main() {
    let config = make_config().unwrap();
    start(config).unwrap();
}

#[inline]
fn make_config() -> io::Result<Config> {
    let mut args_config = Config::from_args();

    let maybe_config_path = args_config.config.as_ref()
        .map(PathBuf::from)
        .or_else(|| xdg::BaseDirectories::with_prefix(env!("CARGO_PKG_NAME"))
            .ok()
            .and_then(|xdg| xdg.find_config_file(&concat!(env!("CARGO_PKG_NAME"), ".toml")))
        )
        .or_else(|| xdg::BaseDirectories::new()
            .ok()
            .and_then(|xdg| xdg.find_config_file(&concat!(env!("CARGO_PKG_NAME"), ".toml")))
        )
        .or_else(|| env::var("HOME")
            .ok()
            .and_then(|home| {
                let path = Path::new(&home).join(concat!(env!("CARGO_PKG_NAME"), ".toml"));
                if path.is_file() {
                    Some(path)
                } else {
                    None
                }
            })
        );

    if let Some(ref path) = maybe_config_path {
        let mut buff = Vec::new();
        File::open(path)?.read_to_end(&mut buff)?;
        let config = toml::from_slice::<Config>(&buff)
            .map_err(|err| err!(Other, err))?;

        if args_config.addr.is_none() {
            args_config.addr = config.addr;
        }
        if args_config.root.is_none() {
            args_config.root = config.root
                .map(|p| path.with_file_name(p).to_string_lossy().into_owned());
        }
        if args_config.cert.is_none() {
            args_config.cert = config.cert
                .map(|p| path.with_file_name(p).to_string_lossy().into_owned());
        }
        if args_config.key.is_none() {
            args_config.key = config.key
                .map(|p| path.with_file_name(p).to_string_lossy().into_owned());
        }
        if args_config.session_buff.is_none() {
            args_config.session_buff = config.session_buff;
        }
    }

    Ok(args_config)
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
