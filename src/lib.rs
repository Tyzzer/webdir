#![feature(proc_macro)]

#[macro_use] extern crate failure;
#[macro_use] extern crate slog;
#[cfg(feature = "sendfile")] extern crate nix;
#[cfg(feature = "sendfile")] extern crate mio;
#[cfg(feature = "sendfile")] extern crate tokio;
extern crate futures;
extern crate tokio_io;
extern crate tokio_threadpool;
extern crate hyper;
extern crate maud;
extern crate percent_encoding;
extern crate byteorder;
extern crate chrono;
extern crate unbytify;
extern crate humanesort;
extern crate mime_guess;
extern crate siphasher;
extern crate base64;
extern crate smallvec;

#[macro_use] mod utils;
#[cfg(feature = "sendfile")] pub mod sendfile;
pub mod error;
mod file;
mod response;
mod process;

use std::io;
use std::sync::Arc;
use std::path::PathBuf;
use futures::future::{ self, FutureResult };
use tokio_threadpool::Sender as PoolSender;
use hyper::{ header, Get, Head, StatusCode };
use hyper::server::{ Service, Request, Response };
use slog::Logger;
use process::Process;

#[cfg(feature = "sendfile")] use tokio::net::TcpStream;
#[cfg(feature = "sendfile")] use futures::sync::BiLock;


pub struct Httpd {
    pub remote: PoolSender,
    pub root: Arc<PathBuf>,
    pub log: Logger,
    pub chunk_length: usize,

    #[cfg(feature = "sendfile")] pub socket: Option<Arc<BiLock<TcpStream>>>,
    #[cfg(feature = "sendfile")] pub use_sendfile: bool
}

impl Service for Httpd {
    type Request = Request;
    type Response = Response;
    type Error = hyper::Error;
    type Future = FutureResult<Self::Response, Self::Error>;

    fn call(&self, req: Request) -> Self::Future {
        info!(self.log, "request";
            "path" => req.path(),
            "method" => format_args!("{}", req.method())
        );
        debug!(self.log, "request"; "headers" => format_args!("{:?}", req.headers()));

        if ![Get, Head].contains(req.method()) {
            return future::ok(
                response::fail(&self.log, true, StatusCode::MethodNotAllowed, &err!(Other, "Not method"))
                    .with_header(header::Allow(vec![Head, Get]))
            );
        }

        match Process::new(self, &self.log, &req).process() {
            Ok(res) => future::ok(res),
            Err(err) => future::ok(response::fail(
                &self.log,
                req.method() != &Head,
                match err.kind() {
                    io::ErrorKind::NotFound => StatusCode::NotFound,
                    io::ErrorKind::PermissionDenied => StatusCode::Forbidden,
                    _ => StatusCode::InternalServerError
                },
                &err
            ))
        }
    }
}
