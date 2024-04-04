use std::{ fs, env };
use std::io::Cursor;
use std::sync::Arc;
use std::net::SocketAddr;
use std::path::{ Path, PathBuf };
use argh::FromArgs;
use anyhow::Context;
use futures::future::TryFutureExt;
use tokio::net::TcpListener;
use tokio_rustls::TlsAcceptor;
use tokio_rustls::rustls::ServerConfig;
use tokio_rustls::rustls::pki_types::{ CertificateDer, PrivateKeyDer };
use hyper_util::server::conn::auto::Builder as HttpBuilder;
use tracing::{ info, error };
use webdir::{ WebDir, WebStream };


/// WebDir -- simple web file server
#[derive(FromArgs)]
struct Options {
    /// bind address
    #[argh(option, short = 'b')]
    pub bind: SocketAddr,

    /// root path
    #[argh(option, short = 'r')]
    pub root: Option<PathBuf>,

    /// index
    #[argh(switch, short = 'i')]
    pub index: bool,

    /// enable HTTPS
    #[argh(option)]
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
    let options: Options = argh::from_env();

    tracing_subscriber::fmt()
        .compact()
        .init();

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
    let listener = TcpListener::bind(&options.bind).await?;
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

            info!(?addr, "peer");

            let stream = WebStream::new(socket, acceptor).await?;
            let stream = hyper_util::rt::tokio::TokioIo::new(stream);

            http_builder
                .serve_connection(stream, webdir)
                .await
                .map_err(|err| anyhow::format_err!("http serve: {:?}", err))?;

            Ok(()) as anyhow::Result<()>
        }.unwrap_or_else(|err| error!(?err, "socket/err"));

        tokio::spawn(fut);
    }
}
