#![feature(plugin)]
#![plugin(maud_macros)]

#[macro_use] extern crate error_chain;
extern crate url;
extern crate bytes;
extern crate futures;
extern crate tokio_io;
extern crate tokio_core;
extern crate hyper;
extern crate maud;
extern crate chrono;
extern crate humansize;

#[macro_use] mod utils;
pub mod error;
mod render;
mod pages;

use std::{ io, env };
use std::path::PathBuf;
use std::sync::Arc;
use futures::future::{ self, FutureResult };
use tokio_core::reactor::Handle;
use hyper::{ header, Get, StatusCode };
use hyper::server::{ Service, Request, Response };


#[derive(Debug, Clone)]
pub struct Httpd {
    pub handle: Handle,
    pub root: Arc<PathBuf>
}

impl Service for Httpd {
    type Request = Request;
    type Response = Response;
    type Error = hyper::Error;
    type Future = FutureResult<Self::Response, Self::Error>;

    fn call(&self, req: Request) -> Self::Future {
        if req.method() != &Get {
            return future::ok(
                pages::fail(StatusCode::MethodNotAllowed, None)
                    .with_header(header::Allow(vec![Get]))
            );
        }

        match pages::process(self, &req) {
            Ok(res) => future::ok(res),
            Err(err) => future::ok(match err.kind() {
                io::ErrorKind::NotFound =>
                    pages::fail(StatusCode::NotFound, None),
                io::ErrorKind::PermissionDenied =>
                    pages::fail(StatusCode::Forbidden, None),
                _ =>
                    pages::fail(StatusCode::InternalServerError, Some(err))
            })
        }
    }
}

impl Httpd {
    pub fn new(handle: &Handle) -> io::Result<Self> {
        Ok(Httpd {
            handle: handle.clone(),
            root: Arc::new(env::current_dir()?)
        })
    }

    pub fn with_root_path<P: Into<PathBuf>>(mut self, path: P) -> Self {
        self.root = Arc::new(path.into());
        self
    }
}
