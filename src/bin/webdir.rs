#![cfg_attr(feature = "sysalloc", feature(alloc_system, global_allocator, allocator_api))]
#![feature(attr_literals, ip_constructors, option_filter)]

#[cfg(feature = "sysalloc")] extern crate alloc_system;
#[cfg(unix)] extern crate xdg;
#[macro_use] extern crate slog;
extern crate slog_term;
extern crate slog_async;
#[macro_use] extern crate structopt;
#[macro_use] extern crate serde_derive;
extern crate toml;
extern crate futures;
extern crate tokio;
extern crate hyper;
#[cfg(feature = "tls")] extern crate rustls;
#[cfg(feature = "tls")] extern crate tokio_rustls;
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
use tokio::net::TcpListener;
use tokio::runtime::Runtime;
use hyper::Chunk;
use hyper::server::Http;
use webdir::Httpd;
use utils::{ Format, read_config, init_logging };

#[cfg(feature = "tls")] use rustls::{ ServerConfig, NoClientAuth, Ticketer };
#[cfg(feature = "tls")] use tokio_rustls::ServerConfigExt;
#[cfg(feature = "tls")] use utils::{ load_certs, load_keys };
#[cfg(feature = "tls")] use hyper::error::Error as HyperError;

#[cfg(feature = "sysalloc")]
#[global_allocator]
static GLOBAL: alloc_system::System = alloc_system::System;


#[derive(StructOpt, Deserialize)]
pub struct Config {
    /// bind address
    #[structopt(short="b", long="bind", display_order=1)]
    pub addr: Option<SocketAddr>,

    /// root path
    #[structopt(short="r", long="root", display_order=2, parse(from_os_str))]
    pub root: Option<PathBuf>,

    /// index
    #[serde(default)]
    #[structopt(short="i", long="index", display_order=3)]
    pub index: bool,

    /// TLS certificate
    #[cfg(feature = "tls")]
    #[structopt(long="cert", requires="key", parse(from_os_str))]
    pub cert: Option<PathBuf>,

    /// TLS key
    #[cfg(feature = "tls")]
    #[structopt(long="key", requires="cert", parse(from_os_str))]
    pub key: Option<PathBuf>,

    /// chunk length
    #[structopt(long="chunk-length", value_name="length", conflicts_with="use_sendfile")]
    pub chunk_length: Option<usize>,

    /// logging format
    #[structopt(long="log-format", value_name="FORMAT", possible_value="COMPACT", possible_value="FULL")]
    pub log_format: Option<Format>,

    /// logging output
    #[structopt(long="log-output", value_name="PATH", parse(from_os_str))]
    pub log_output: Option<PathBuf>,

    /// logging level
    #[structopt(
        long="log-level",
        value_name="LEVEL",
        possible_value="OFF", possible_value="CRITICAL", possible_value="ERROR",
        possible_value="WARN", possible_value="INFO", possible_value="DEBUG", possible_value="TRACE"
    )]
    pub log_level: Option<String>,

    /// read config from file
    #[serde(skip_serializing)]
    #[structopt(short="c", long="config", parse(from_os_str))]
    pub config: Option<PathBuf>,

    /// disable keepalive
    #[serde(skip_serializing, default)]
    #[structopt(long="no-keepalive")]
    pub no_keepalive: bool,

    #[structopt(hidden=true)]
    pub keepalive: Option<bool>,

    /// use sendfile
    #[cfg(feature = "sendfile")]
    #[serde(skip_serializing, default)]
    #[structopt(long="use-sendfile")]
    pub use_sendfile: bool,

    #[cfg(feature = "sendfile")]
    #[structopt(hidden=true)]
    pub sendfile: Option<bool>,
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
        merge_config!(chunk_length);
        merge_config!(log_format);
        merge_config!(log_output);
        merge_config!(log_level);
        merge_config!(root -> |p| path.with_file_name(p));

        #[cfg(feature = "tls")] merge_config!(cert -> |p| path.with_file_name(p));
        #[cfg(feature = "tls")] merge_config!(key -> |p| path.with_file_name(p));

        args_config.keepalive =
            if args_config.no_keepalive { Some(false) }
            else { config.keepalive };

        #[cfg(feature = "sendfile")] {
            args_config.sendfile =
                if args_config.use_sendfile { Some(true) }
                else { config.sendfile };
        }

        args_config.index |= config.index;
    }

    Ok(args_config)
}


#[cfg_attr(not(feature = "tls"), allow(unused_variables, unused_mut))]
#[inline]
fn start(mut config: Config) -> io::Result<()> {
    let log = init_logging(&config)?;

    #[cfg(feature = "tls")]
    let maybe_tls_config = if let (Some(cert), Some(key)) = (config.cert.take(), config.key.take()) {
        let mut tls_config = ServerConfig::new(NoClientAuth::new());
        tls_config.ticketer = Ticketer::new();
        tls_config.set_single_cert(load_certs(&cert)?, load_keys(&key)?.remove(0));
        Some(Arc::new(tls_config))
    } else {
        None
    };

    #[cfg(not(feature = "tls"))]
    let maybe_tls_config: Option<()> = None;

    let root =
        if let Some(ref p) = config.root { Arc::new(Path::new(p).canonicalize()?) }
        else { Arc::new(env::current_dir()?) };
    let addr = config.addr.unwrap_or_else(|| SocketAddr::new(IpAddr::V4(Ipv4Addr::localhost()), 0));
    let index = config.index;
    let keepalive = config.keepalive.unwrap_or(true);
    let chunk_length = config.chunk_length.unwrap_or(1 << 16);

    #[cfg(feature = "sendfile")]
    let sendfile_flag = maybe_tls_config.is_none() && config.sendfile.unwrap_or(false);

    drop(config);

    let mut rt = Runtime::new()?;
    let executor = rt.executor();
    let listener = TcpListener::bind(&addr)?;

    info!(log, "listening";
        "root" => format_args!("{}", root.display()),
        "addr" => format_args!("{}", listener.local_addr()?),
        "keepalive" => keepalive,
        "tls" => maybe_tls_config.is_some()
    );

    let done = listener.incoming().for_each(move |stream| {
        let log = log.new(o!("addr" => format!("{:?}", stream.peer_addr())));
        let httpd = Httpd {
            remote: executor.clone(),
            root: Arc::clone(&root),
            log: log.clone(),
            chunk_length, index,
            #[cfg(feature = "sendfile")] socket: None,
            #[cfg(feature = "sendfile")] use_sendfile: sendfile_flag
        };

        if let Some(ref tls_config) = maybe_tls_config {
            #[cfg(feature = "tls")]
            let done = tls_config.accept_async(stream)
                .map_err(HyperError::Io)
                .and_then(move |stream| Http::<Chunk>::new()
                    .keep_alive(keepalive)
                    .serve_connection(stream, httpd)
                )
                .map(drop)
                .map_err(move |err| error!(log, "http/tls"; "err" => format_args!("{}", err)));

            #[cfg(feature = "tls")]
            executor.spawn(done);
        } else {
            #[cfg(feature = "sendfile")]
            let done = {
                use futures::sync::BiLock;
                use webdir::sendfile::BiTcpStream;

                let mut httpd = httpd;
                let (stream, stream2) = BiLock::new(stream);
                httpd.socket = Some(Arc::new(stream2));

                stream.lock()
                    .map(BiTcpStream)
                    .and_then(move |stream| Http::<Chunk>::new()
                        .keep_alive(keepalive)
                        .serve_connection(stream, httpd)
                        .map(drop)
                        .map_err(move |err| error!(log, "http"; "err" => format_args!("{}", err)))
                    )
            };

            #[cfg(not(feature = "sendfile"))]
            let done = Http::<Chunk>::new()
                .keep_alive(keepalive)
                .serve_connection(stream, httpd)
                .map(drop)
                .map_err(move |err| error!(log, "http"; "err" => format_args!("{}", err)));

            executor.spawn(done);
        }

        Ok(())
    });

    rt.spawn(done.map_err(|err| eprintln!("Error: {:?}", err)));
    let _ = rt.shutdown_on_idle().wait();

    Ok(())
}

fn main() {
    let config = make_config().unwrap();
    start(config).unwrap();
}
