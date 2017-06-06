#![feature(plugin, sort_unstable)]
#![plugin(maud_macros)]

#[macro_use] extern crate error_chain;
#[macro_use] extern crate slog;
extern crate url;
extern crate bytes;
extern crate futures;
extern crate tokio_io;
extern crate tokio_core;
extern crate hyper;
extern crate maud;
extern crate chrono;
extern crate humansize;
extern crate humanesort;
extern crate mime_guess;
extern crate metrohash;
extern crate data_encoding;
extern crate smallvec;

#[macro_use] mod utils;
pub mod error;
mod response;
mod process;

use std::io;
use std::sync::Arc;
use std::path::PathBuf;
use futures::future::{ self, FutureResult };
use tokio_core::reactor::Remote;
use hyper::{ header, Get, Head, StatusCode };
use hyper::server::{ Service, Request, Response };
use slog::Logger;
use process::Process;


#[derive(Debug, Clone)]
pub struct Httpd {
    pub handle: Remote,
    pub root: Arc<PathBuf>,
    pub log: Logger
}

impl Service for Httpd {
    type Request = Request;
    type Response = Response;
    type Error = hyper::Error;
    type Future = FutureResult<Self::Response, Self::Error>;

    fn call(&self, req: Request) -> Self::Future {
        if ![Get, Head].contains(req.method()) {
            return future::ok(
                response::fail(&self.log, false, StatusCode::MethodNotAllowed, &err!(Other, "Not method"))
                    .with_header(header::Allow(vec![Get]))
            );
        }

        info!(self.log, "request";
            "path" => req.path(),
            "method" => format_args!("{}", req.method())
        );

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

impl Httpd {
    #[inline]
    pub fn new(handle: Remote, log: Logger, root: Arc<PathBuf>) -> Self {
        Httpd { handle, root, log }
    }
}
