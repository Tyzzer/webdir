#![feature(never_type)]

#[macro_use]
mod common;
mod stream;
mod file;
mod process;

use std::io;
use std::task::{ Context, Poll };
use std::sync::Arc;
use std::path::Path;
use futures::future;
use hyper::service::Service;
use hyper::{ StatusCode, Request, Response, Body };
use log::*;
use crate::process::Process;
use crate::common::err_html;
pub use crate::stream::Stream as WebStream;

#[derive(Clone)]
pub struct WebDir {
    pub root: Arc<Path>,
    pub index: bool,
}

impl WebDir {
    pub fn new(root: Arc<Path>, index: bool) -> io::Result<Self> {
        Ok(WebDir { root, index })
    }
}

impl Service<Request<Body>> for WebDir {
    type Response = Response<Body>;
    type Error = !;
    type Future = future::Ready<Result<Response<Body>, Self::Error>>;

    fn poll_ready(&mut self, _cx: &mut Context) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, req: Request<Body>) -> Self::Future {
        info!("request: {} {}", req.method(), req.uri().path());
        debug!("request: {:?}", req.headers());

        match Process::new(self, req).process() {
            Ok(resp) => future::ok(resp),
            Err(err) => {
                let body = err_html(format_args!("{:?}", err)).into_string();
                let mut resp = Response::new(Body::from(body));

                match err.kind() {
                    io::ErrorKind::NotFound => *resp.status_mut() =  StatusCode::NOT_FOUND,
                    io::ErrorKind::PermissionDenied => *resp.status_mut() =  StatusCode::FORBIDDEN,
                    _ => *resp.status_mut() = StatusCode::INTERNAL_SERVER_ERROR
                }

                future::ok(resp)
            }
        }
    }
}
