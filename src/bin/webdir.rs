use std::{ fs, env };
use std::io::Cursor;
use std::sync::Arc;
use std::net::SocketAddr;
use std::path::{ Path, PathBuf };
use structopt::StructOpt;
use anyhow::Context;
use futures::future::TryFutureExt;
use tokio::net::TcpListener;
use tokio_rustls::TlsAcceptor;
use tokio_rustls::rustls::ServerConfig;
use tokio_rustls::rustls::pki_types::{ CertificateDer, PrivateKeyDer };
use hyper_util::server::conn::auto::Builder as HttpBuilder;
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

fn load_cert_and_key(path: &Path)
    -> anyhow::Result<(Vec<CertificateDer<'static>>, PrivateKeyDer<'static>)>
{
    let mut reader = Cursor::new(fs::read(path)?);

    let cert = rustls_pemfile::certs(&mut reader)
        .collect::<Result<Vec<_>, _>>()
        .context("Bad certs")?;

    reader.set_position(0);
    let key = rustls_pemfile::private_key(&mut reader)
        .context("Bad private key")?
        .context("not found private key")?;

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
        let (certs, key) = load_cert_and_key(cert)?;
        let mut config = ServerConfig::builder()
            .with_no_client_auth()
            .with_single_cert(certs, key)?;
        config.alpn_protocols = vec!["h2".into(), "http/1.1".into()];
        let config = Arc::new(config);
        Some(TlsAcceptor::from(config))
    } else {
        None
    };

    let webdir = WebDir::new(root, options.index)?;
    let listener = TcpListener::bind(&options.addr).await?;
    let mut http_builder = HttpBuilder::new(hyper_util::rt::tokio::TokioExecutor::new());
    http_builder
        .http1()
        .half_close(true)
        .max_buf_size(2048 * 1024)
        .timer(hyper_util::rt::tokio::TokioTimer::new())
        .http2()
        .adaptive_window(true)
        .max_send_buf_size(2048 * 1024)
        .timer(hyper_util::rt::tokio::TokioTimer::new());
    let http_builder = Arc::new(http_builder);

    info!("bind: {:?}", listener.local_addr());

    loop {
        let result = listener.accept().await;
        let webdir = webdir.clone();
        let acceptor = acceptor.clone();
        let http_builder = http_builder.clone();

        let fut = async move {
            let (socket, addr) = result?;

            info!("addr: {:?}", addr);

            let stream = WebStream::new(socket, acceptor).await?;
            let stream = hyper_util::rt::tokio::TokioIo::new(stream);

            http_builder
                .serve_connection(stream, webdir)
                .await
                .map_err(|err| anyhow::format_err!("http serve error: {:?}", err))?;

            Ok(()) as anyhow::Result<()>
        }.unwrap_or_else(|err| error!("socket/err: {:?}", err));

        tokio::spawn(fut);
    }
}
