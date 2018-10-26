#![feature(read_initializer, never_type, proc_macro_hygiene)]

#[macro_use]
pub mod common;
pub mod stream;
pub mod file;
mod process;

use std::{ io, fs };
use std::sync::Arc;
use std::path::PathBuf;
use tokio::prelude::*;
use hyper::service::Service;
use hyper::{ StatusCode, Method, Request, Response, Body };
use log::{ log, info, debug };
use crate::process::Process;
use crate::common::err_html;


#[derive(Clone)]
pub struct WebDir {
    pub root: Arc<PathBuf>,
    pub index: bool
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
                    _ => *resp.status_mut() =  StatusCode::INTERNAL_SERVER_ERROR
                }

                future::ok(resp)
            }
        }
    }
}
