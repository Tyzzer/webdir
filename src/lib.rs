#![feature(read_initializer, never_type, proc_macro_hygiene)]

#[macro_use]
mod common;
mod stream;
mod file;
mod process;

#[cfg(target_os = "linux")]
mod aio;

use std::io;
use std::sync::Arc;
use std::path::PathBuf;
use tokio::prelude::*;
use hyper::service::Service;
use hyper::{ StatusCode, Request, Response, Body };
use log::*;
use crate::process::Process;
use crate::common::err_html;
pub use crate::stream::Stream as WebStream;


#[derive(Clone)]
pub struct WebDir {
    pub root: Arc<PathBuf>,
    pub index: bool,

    #[cfg(target_os = "linux")]
    context: tokio_linux_aio::AioContext
}

impl WebDir {
    #[cfg(not(target_os = "linux"))]
    pub fn new(root: Arc<PathBuf>, index: bool) -> io::Result<Self> {
        Ok(WebDir { root, index })
    }

    #[cfg(target_os = "linux")]
    pub fn new(root: Arc<PathBuf>, index: bool) -> io::Result<Self> {
        use tokio_linux_aio::AioContext;
        use crate::aio::HyperExecutor;

        static EXECUTOR: &'static HyperExecutor = &HyperExecutor;

        let context = AioContext::new(EXECUTOR, 10)?;
        Ok(WebDir { root, index, context })
    }
}

impl Service for WebDir {
    type ReqBody = Body;
    type ResBody = Body;
    type Error = !;
    type Future = future::FutureResult<Response<Self::ResBody>, Self::Error>;

    fn call(&mut self, req: Request<Self::ReqBody>) -> Self::Future {
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
