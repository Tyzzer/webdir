#[macro_use] extern crate error_chain;
extern crate libc;
extern crate bytes;
extern crate futures;
extern crate tokio_io;
extern crate tokio_core;
extern crate hyper;

#[macro_use] mod utils;
pub mod aio;

use std::io;
use std::fs::File;
use std::path::Path;
use futures::Stream;
use futures::future::{ self, FutureResult };
use tokio_core::reactor::Handle;
use hyper::{ Get, Post, StatusCode };
use hyper::header::ContentLength;
use hyper::server::{ Service, Request, Response };
use utils::path_canonicalize;
use aio::AioReadBuf;


#[derive(Debug, Clone)]
pub struct Httpd;

impl Service for Httpd {
    type Request = Request;
    type Response = Response<AioReadBuf>;
    type Error = hyper::Error;
    type Future = FutureResult<Self::Response, Self::Error>;

    fn call(&self, req: Request) -> Self::Future {
        let target_path = path_canonicalize(req.path());

        match target_path.metadata() {
            Ok(metadata) => if let Ok(dir) = target_path.read_dir() {
                unimplemented!()
            } else if let Ok(fd) = File::open(&target_path) {
                let res = Response::new()
                    .with_header(ContentLength(metadata.len()))
                    .with_body(AioReadBuf::new(fd, 0..metadata.len() as usize));
                future::ok(res)
            } else {
                unimplemented!()
            },
            Err(err) => match err.kind() {
                io::ErrorKind::NotFound => {
                    /* TODO 404 */
                    unimplemented!()
                },
                io::ErrorKind::PermissionDenied => {
                    /* TODO 403 */
                    unimplemented!()
                },
                _ => {
                    /* TODO 500 */
                    unimplemented!()
                }
            }
        }
    }
}

impl Httpd {
    pub fn new() -> Self {
        Httpd
    }
}
