#![feature(plugin, sort_unstable)]
#![plugin(maud_macros)]

#[macro_use] extern crate error_chain;
#[macro_use] extern crate slog;
extern crate url;
extern crate nix;
extern crate libc;
extern crate bytes;
extern crate futures;
extern crate tokio_io;
extern crate tokio_core;
extern crate hyper;
extern crate maud;
extern crate chrono;
extern crate humansize;
extern crate humanesort;
extern crate mime;
extern crate mime_guess;
extern crate metrohash;
extern crate data_encoding;
extern crate smallvec;

#[macro_use] mod utils;
pub mod error;
mod file;
mod sendfile;
mod response;
mod process;

use std::io;
use std::sync::Arc;
use std::path::PathBuf;
use futures::future::{ self, FutureResult };
use futures::sync::BiLock;
use tokio_core::net::TcpStream;
use tokio_core::reactor::Remote;
use hyper::{ header, Get, Head, StatusCode };
use hyper::server::{ Service, Request, Response };
use slog::Logger;
use process::Process;
pub use sendfile::BiStream;


pub struct Httpd {
    pub remote: Remote,
    pub root: Arc<PathBuf>,
    pub log: Logger,
    pub socket: Option<Arc<BiLock<TcpStream>>>
}

impl Service for Httpd {
    type Request = Request;
    type Response = Response;
    type Error = hyper::Error;
    type Future = FutureResult<Self::Response, Self::Error>;

    fn call(&self, req: Request) -> Self::Future {
        if ![Get, Head].contains(req.method()) {
            return future::ok(
                response::fail(&self.log, true, StatusCode::MethodNotAllowed, &err!(Other, "Not method"))
                    .with_header(header::Allow(vec![Head, Get]))
            );
        }

        info!(self.log, "request";
            "path" => req.path(),
            "method" => format_args!("{}", req.method())
        );
        debug!(self.log, "request"; "headers" => format_args!("{:?}", req.headers()));

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
