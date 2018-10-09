#![feature(read_initializer)]

pub mod common;
pub mod stream;
pub mod file;
mod process;

use std::fs;
use std::sync::Arc;
use std::path::PathBuf;
use tokio::prelude::*;
use hyper::service::Service;
use hyper::{ Method, Request, Response, Body };
use crate::process::Process;


#[derive(Clone)]
pub struct WebDir {
    pub root: Arc<PathBuf>,
    pub index: bool
}

impl Service for WebDir {
    type ReqBody = Body;
    type ResBody = Body;
    type Error = hyper::Error;
    type Future = future::FutureResult<Response<Self::ResBody>, Self::Error>;

    fn call(&mut self, req: Request<Self::ReqBody>) -> Self::Future {
        match Process::new(self, &req).process() {
            Ok(resp) => future::ok(resp),
            Err(err) => {
                let resp = Response::new(Body::from("err"));
                future::ok(resp)
            }
        }
    }
}
