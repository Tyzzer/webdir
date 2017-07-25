#![cfg_attr(feature = "sysalloc", feature(alloc_system))]
#![feature(attr_literals)]

#[cfg(feature = "sysalloc")] extern crate alloc_system;
#[macro_use] extern crate slog;
extern crate slog_term;
extern crate slog_async;
#[macro_use] extern crate structopt_derive;
extern crate structopt;
#[macro_use] extern crate serde_derive;
extern crate serde;
extern crate xdg;
extern crate toml;
extern crate futures;
extern crate hyper;
extern crate tokio_core;
extern crate rustls;
extern crate tokio_rustls;
#[macro_use] extern crate webdir;

mod utils;

use std::{ env, io };
use std::fs::File;
use std::io::Read;
use std::sync::Arc;
use std::net::{ SocketAddr, IpAddr, Ipv4Addr };
use std::path::{ Path, PathBuf };
use structopt::StructOpt;
use futures::{ Future, Stream };
use hyper::server::Http;
use tokio_core::reactor::Core;
use tokio_core::net::TcpListener;
use rustls::{ ServerConfig, ServerSessionMemoryCache };
use tokio_rustls::ServerConfigExt;
use webdir::Httpd;
use utils::{
    Format,
    read_config, init_logging, load_certs, load_keys
};


#[derive(StructOpt, Deserialize)]
#[structopt]
pub struct Config {
    /// bind address
    #[structopt(short = "b", long = "bind", display_order = 1)]
    pub addr: Option<SocketAddr>,

    /// root path
    #[structopt(short = "r", long = "root", display_order = 2)]
    pub root: Option<String>,

    /// TLS certificate
    #[structopt(long = "cert", requires = "key", display_order = 3)]
    pub cert: Option<String>,
    /// TLS key
    #[structopt(long = "key", requires = "cert", display_order = 4)]
    pub key: Option<String>,
    /// TLS session buffer size
    #[structopt(long = "session-buff", requires = "cert", display_order = 5)]
    pub session_buff_size: Option<usize>,

    /// chunk length
    #[structopt(long = "chunk-length", display_order = 6)]
    pub chunk_length: Option<usize>,

    /// logging format
    #[structopt(short = "f", long = "log-format", display_order = 7, possible_value = "compact", possible_value = "full")]
    pub format: Option<Format>,

    /// logging output
    #[structopt(short = "o", long = "log-output", display_order = 8)]
    pub log_output: Option<String>,

    /// read config from file
    #[serde(skip_serializing)]
    #[structopt(short = "c", long = "config", display_order = 9)]
    pub config: Option<String>,

    /// disable keepalive
    #[serde(skip_serializing, default)]
    #[structopt(long = "no-keepalive")]
    pub no_keepalive: bool,

    #[structopt(hidden = true)]
    pub keepalive: Option<bool>
}


#[inline]
fn make_config() -> io::Result<Config> {
    let mut args_config = Config::from_args();

    let maybe_config_path = args_config.config.as_ref()
        .map(PathBuf::from)
        .or_else(read_config);

    if let Some(ref path) = maybe_config_path {
        let mut buff = Vec::new();
        File::open(path)?.read_to_end(&mut buff)?;
        let config = toml::from_slice::<Config>(&buff)
            .map_err(|err| err!(Other, err))?;

        macro_rules! merge_config {
            ( $option:ident -> $block:expr ) => {
                if args_config.$option.is_none() {
                    args_config.$option = config.$option
                        .map($block);
                }
            };
            ( $option:ident ) => {
                if args_config.$option.is_none() {
                    args_config.$option = config.$option;
                }
            };
        }

        merge_config!(addr);
        merge_config!(session_buff_size);
        merge_config!(chunk_length);
        merge_config!(format);
        merge_config!(log_output);
        merge_config!(root -> |p| path.with_file_name(p).to_string_lossy().into_owned());
        merge_config!(cert -> |p| path.with_file_name(p).to_string_lossy().into_owned());
        merge_config!(key -> |p| path.with_file_name(p).to_string_lossy().into_owned());

        if args_config.no_keepalive {
            args_config.keepalive = Some(false);
        } else {
            args_config.keepalive = config.keepalive;
        }
    }

    Ok(args_config)
}


#[inline]
fn start(config: Config) -> io::Result<()> {
    let log = init_logging(&config)?;

    let maybe_tls_config = if let (Some(ref cert), Some(ref key)) = (config.cert, config.key) {
        let mut tls_config = ServerConfig::new();
        tls_config.set_single_cert(load_certs(cert)?, load_keys(key)?.remove(0));
        tls_config.set_persistence(ServerSessionMemoryCache::new(config.session_buff_size.unwrap_or(64)));
        Some(Arc::new(tls_config))
    } else {
        None
    };

    let root =
        if let Some(ref p) = config.root { Arc::new(Path::new(p).canonicalize()?) }
        else { Arc::new(env::current_dir()?) };
    let addr = config.addr.unwrap_or_else(|| SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 0));
    let keepalive = config.keepalive.unwrap_or(true);
    let chunk_length = config.chunk_length.unwrap_or(1 << 16);

    let mut core = Core::new()?;
    let handle = core.handle();
    let remote = handle.remote().clone();
    let listener = TcpListener::bind(&addr, &handle)?;

    info!(log, "listening";
        "root" => format_args!("{}", root.display()),
        "addr" => format_args!("{}", listener.local_addr()?),
        "keepalive" => keepalive,
        "tls" => maybe_tls_config.is_some()
    );

    let done = listener.incoming().for_each(|(stream, addr)| {
        let log = log.new(o!("addr" => format!("{}", addr)));
        let httpd = Httpd {
            remote: remote.clone(),
            root: root.clone(),
            log: log.clone(),
            chunk_length: chunk_length,
            #[cfg(feature = "sendfile")] socket: None
        };

        if let Some(ref tls_config) = maybe_tls_config {
            let handle2 = handle.clone();

            let done = tls_config.accept_async(stream)
                .map(move |stream| Http::new()
                    .keep_alive(keepalive)
                    .bind_connection(&handle2, stream, addr, httpd)
                )
                .map_err(move |err| error!(log, "tls"; "err" => format_args!("{}", err)));

            handle.spawn(done);
        } else {
            #[cfg(feature = "sendfile")] {
                use futures::sync::BiLock;
                use webdir::sendfile::BiTcpStream;

                let mut httpd = httpd;
                let (stream, stream2) = BiLock::new(stream);
                let handle2 = handle.clone();
                httpd.socket = Some(Arc::new(stream2));

                let done = stream.lock()
                    .map(BiTcpStream)
                    .map(move |stream| Http::new()
                        .keep_alive(keepalive)
                        .bind_connection(&handle2, stream, addr, httpd)
                    );

                handle.spawn(done);
            };

            #[cfg(not(feature = "sendfile"))]
            Http::new()
                .keep_alive(keepalive)
                .bind_connection(&handle, stream, addr, httpd);
        }

        Ok(())
    });

    core.run(done)
}

fn main() {
    let config = make_config().unwrap();
    start(config).unwrap();
}
