#![feature(never_type)]

#[macro_use]
extern crate tracing;

#[macro_use]
mod utils;
mod stream;
mod process;
mod file;
mod body;

use std::io;
use std::sync::Arc;
use std::path::Path;
use futures::future;
use hyper::body::Incoming;
use hyper::service::Service;
use hyper::{ StatusCode, Request, Response};
use crate::body::ResponseBody as Body;
use crate::process::Process;
use crate::utils::err_html;
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

impl Service<Request<Incoming>> for WebDir {
    type Response = Response<Body>;
    type Error = !;
    type Future = future::Ready<Result<Response<Body>, Self::Error>>;

    fn call(&self, req: Request<Incoming>) -> Self::Future {
        info!(method=%req.method(), path=%req.uri().path(), "request");
        debug!(headers=?req.headers(), "request headers");

        match Process::new(self, req).process() {
            Ok(resp) => future::ok(resp),
            Err(err) => {
                let body = err_html(format_args!("{:?}", err)).into_string();
                let mut resp = Response::new(Body::one(body.into()));

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
