#![feature(never_type)]

use std::{ fs, env };
use std::io::Cursor;
use std::sync::Arc;
use std::net::SocketAddr;
use std::path::{ Path, PathBuf };
use structopt::StructOpt;
use failure::Fallible;
use rustls::{ PrivateKey, Certificate, ServerConfig, NoClientAuth, Ticketer };
use tokio::prelude::*;
use tokio::net::TcpListener;
use tokio_rustls::TlsAcceptor;
use hyper::server::conn::Http;
use webdir::{ WebDir, stream::Stream as WebStream };


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

fn load_cert_and_key(path: &Path) -> Fallible<(Vec<Certificate>, Vec<PrivateKey>)> {
    use rustls::internal::pemfile::{ certs, rsa_private_keys, pkcs8_private_keys };

    let mut reader = Cursor::new(fs::read(path)?);

    let cert = certs(&mut reader)
        .map_err(|_| failure::err_msg("Bad certs"))?;
    reader.set_position(0);
    let mut key = rsa_private_keys(&mut reader)
        .map_err(|_| failure::err_msg("Bad rsa privatek key"))?;
    reader.set_position(0);
    let mut key2 = pkcs8_private_keys(&mut reader)
        .map_err(|_| failure::err_msg("Bad pkcs8 privatek key"))?;
    key.append(&mut key2);

    Ok((cert, key))
}


fn main() -> Fallible<()> {
    let options = Options::from_args();

    let root =
        if let Some(ref p) = options.root { Arc::new(p.canonicalize()?) }
        else { Arc::new(env::current_dir()?) };

    let acceptor = if let Some(cert) = options.https.as_ref() {
        let (certs, mut keys) = load_cert_and_key(cert)?;
        let key = keys.pop()
            .ok_or_else(|| failure::err_msg("not found keys"))?;

        let mut config = ServerConfig::new(NoClientAuth::new());
        config.set_single_cert(certs, key)?;
        config.ticketer = Ticketer::new();

        // ktls first
        config.ignore_client_order = true;
        let cipher = config.ciphersuites.remove(6);
        config.ciphersuites.insert(0, cipher);
        let cipher = config.ciphersuites.remove(8);
        config.ciphersuites.insert(1, cipher);

        let config = Arc::new(config);

        // TODO

        Some(TlsAcceptor::from(config))
    } else {
        None
    };

    let listener = TcpListener::bind(&options.addr)?;

    println!("bind: {:?}", listener.local_addr());

    let webdir = WebDir { root, index: options.index };

    let done = listener.incoming().for_each(move |socket| {
        let webdir = webdir.clone();

        let fut = WebStream::new(socket, acceptor.clone())
            .map_err(failure::Error::from)
            .and_then(move |stream| Http::new()
                .serve_connection(stream, webdir)
                .map_err(Into::into)
            )
            .map(drop)
            .map_err(|err| eprintln!("err: {:?}", err));

        hyper::rt::spawn(fut);
        Ok(())
    })
        .map_err(|err| eprintln!("err: {:?}", err));

    hyper::rt::run(done);
    Ok(())
}
