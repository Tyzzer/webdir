#![feature(plugin)]
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

#[macro_use] mod utils;
pub mod error;
mod sortdir;
mod render;
mod pages;

use std::{ io, env };
use std::path::PathBuf;
use std::sync::Arc;
use futures::future::{ self, FutureResult };
use tokio_core::reactor::Handle;
use hyper::{ header, Get, Head, StatusCode };
use hyper::server::{ Service, Request, Response };
use slog::Logger;


#[derive(Debug, Clone)]
pub struct Httpd {
    pub handle: Handle,
    pub root: Arc<PathBuf>,
    pub log: Logger
}

impl Service for Httpd {
    type Request = Request;
    type Response = Response;
    type Error = hyper::Error;
    type Future = FutureResult<Self::Response, Self::Error>;

    fn call(&self, req: Request) -> Self::Future {
        let log = self.log.new(o!("addr" => format!("{:?}", req.remote_addr())));

        info!(log, "request";
            "path" => req.path(),
            "method" => format_args!("{}", req.method())
        );

        if ![Get, Head].contains(req.method()) {
            return future::ok(
                pages::fail(&log, false, StatusCode::MethodNotAllowed, &err!(Other))
                    .with_header(header::Allow(vec![Get]))
            );
        }

        match pages::process(self, &log, &req) {
            Ok(res) => future::ok(res),
            Err(err) => future::ok(pages::fail(
                &log,
                req.method() == &Head,
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
    pub fn new(handle: Handle, log: Logger) -> io::Result<Self> {
        Ok(Httpd {
            handle: handle,
            root: Arc::new(env::current_dir()?),
            log
        })
    }

    pub fn with_root_path<P: Into<PathBuf>>(mut self, path: P) -> Self {
        self.root = Arc::new(path.into());
        self
    }
}
