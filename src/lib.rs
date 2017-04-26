#[macro_use] extern crate error_chain;
extern crate bytes;
extern crate futures;
extern crate tokio_io;
extern crate tokio_core;
extern crate hyper;

#[macro_use] mod utils;
pub mod error;

use std::io;
use std::fs::File;
use futures::{ Future, Stream };
use futures::future::{ self, FutureResult };
use tokio_core::reactor::Handle;
use hyper::{ Get, Post, StatusCode, Body };
use hyper::header::ContentLength;
use hyper::server::{ Service, Request, Response };
use utils::path_canonicalize;


#[derive(Debug, Clone)]
pub struct Httpd {
    handle: Handle
}

impl Service for Httpd {
    type Request = Request;
    type Response = Response;
    type Error = hyper::Error;
    type Future = FutureResult<Self::Response, Self::Error>;

    fn call(&self, req: Request) -> Self::Future {
        let target_path = path_canonicalize(req.path());

        match target_path.metadata() {
            Ok(metadata) => if let Ok(dir) = target_path.read_dir() {
                unimplemented!()
            } else if let Ok(_fd) = File::open(&target_path) {
                unimplemented!()
            } else {
                unimplemented!()
            },
            Err(err) => match err.kind() {
                io::ErrorKind::NotFound => {
                    // TODO 404
                    unimplemented!()
                },
                io::ErrorKind::PermissionDenied => {
                    // TODO 403
                    unimplemented!()
                },
                _ => {
                    // TODO 500
                    unimplemented!()
                }
            }
        }
    }
}

impl Httpd {
    pub fn new(handle: &Handle) -> Self {
        Httpd {
            handle: handle.clone()
        }
    }
}
