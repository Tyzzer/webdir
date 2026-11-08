use std::env;
use std::sync::Arc;
use std::net::SocketAddr;
use std::path::PathBuf;
use argh::FromArgs;
use futures::future::TryFutureExt;
use tokio::net::TcpListener;
use hyper_util::server::conn::auto::Builder as HttpBuilder;
use tracing::{ info, error };
use webdir::WebDir;


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
        let http_builder = http_builder.clone();

        let fut = async move {
            let (socket, addr) = result?;

            info!(?addr, "peer");

            let stream = hyper_util::rt::tokio::TokioIo::new(socket);

            http_builder
                .serve_connection(stream, webdir)
                .await
                .map_err(|err| anyhow::format_err!("http serve: {:?}", err))?;

            Ok(()) as anyhow::Result<()>
        }.unwrap_or_else(|err| error!(?err, "socket/err"));

        tokio::spawn(fut);
    }
}
