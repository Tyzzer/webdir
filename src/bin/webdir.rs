use std::{ fs, env };
use std::io::Cursor;
use std::sync::Arc;
use std::net::SocketAddr;
use std::path::{ Path, PathBuf };
use structopt::StructOpt;
use rustls::{ PrivateKey, Certificate, ServerConfig, NoClientAuth, Ticketer };
use futures::future::TryFutureExt;
use tokio::net::TcpListener;
use tokio::stream::StreamExt;
use tokio_rustls::TlsAcceptor;
use hyper::server::conn::Http;
use slog::{ slog_o, Drain };
use log::*;
use webdir::{ WebDir, WebStream };


#[derive(StructOpt)]
struct Options {
    /// bind address
    #[structopt(short="b", long="bind")]
    pub addr: SocketAddr,

    /// root path
    #[structopt(short="r", long="root", display_order=2, parse(from_os_str))]
    pub root: Option<PathBuf>,

    /// index
    #[structopt(short="i", long="index", display_order=3)]
    pub index: bool,

    /// enable HTTPS
    #[structopt(long="https", parse(from_os_str))]
    pub https: Option<PathBuf>
}

fn load_cert_and_key(path: &Path) -> anyhow::Result<(Vec<Certificate>, Vec<PrivateKey>)> {
    use rustls::internal::pemfile::{ certs, rsa_private_keys, pkcs8_private_keys };

    let mut reader = Cursor::new(fs::read(path)?);

    let cert = certs(&mut reader)
        .map_err(|_| anyhow::format_err!("Bad certs"))?;
    reader.set_position(0);
    let mut key = rsa_private_keys(&mut reader)
        .map_err(|_| anyhow::format_err!("Bad rsa privatek key"))?;
    reader.set_position(0);
    let mut key2 = pkcs8_private_keys(&mut reader)
        .map_err(|_| anyhow::format_err!("Bad pkcs8 privatek key"))?;
    key.append(&mut key2);

    Ok((cert, key))
}


#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let options = Options::from_args();

    let level = env::var("WEBDIR_LOG")
        .as_ref()
        .map(String::as_str)
        .unwrap_or("INFO")
        .parse()
        .map_err(|_| anyhow::format_err!("bad log level"))?;

    let decorator = slog_term::TermDecorator::new().build();
    let drain = slog_term::CompactFormat::new(decorator).build().fuse();
    let drain = slog::LevelFilter::new(drain, level).fuse();
    let drain = slog_async::Async::new(drain).build().fuse();
    let logger = slog::Logger::root(drain, slog_o!("version" => env!("CARGO_PKG_VERSION")));

    let _scope_guard = slog_scope::set_global_logger(logger);
    slog_stdlog::init()?;

    let root =
        if let Some(ref p) = options.root { Arc::from(&*p.canonicalize()?) }
        else { Arc::from(&*env::current_dir()?) };

    let acceptor = if let Some(cert) = options.https.as_ref() {
        let (certs, mut keys) = load_cert_and_key(cert)?;
        let key = keys.pop()
            .ok_or_else(|| anyhow::format_err!("not found keys"))?;

        let mut config = ServerConfig::new(NoClientAuth::new());
        config.set_single_cert(certs, key)?;
        config.ticketer = Ticketer::new();
        config.alpn_protocols = vec!["h2".into(), "http/1.1".into()];
        let config = Arc::new(config);
        Some(TlsAcceptor::from(config))
    } else {
        None
    };

    let webdir = WebDir::new(root, options.index)?;
    let mut listener = TcpListener::bind(&options.addr).await?;

    info!("bind: {:?}", listener.local_addr());

    let mut incoming = listener.incoming();
    while let Some(socket) = incoming.next().await {
        let webdir = webdir.clone();
        let acceptor = acceptor.clone();

        let fut = async move {
            let socket = socket?;

            info!("addr: {:?}", socket.peer_addr());

            let stream = WebStream::new(socket, acceptor).await?;

            Http::new()
                .serve_connection(stream, webdir).await?;

            Ok(()) as anyhow::Result<()>
        }.unwrap_or_else(|err| error!("socket/err: {:?}", err));

        tokio::spawn(fut);
    }

    Ok(())
}
