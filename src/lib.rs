#![feature(plugin)]
#![plugin(maud_macros)]

#[macro_use] extern crate error_chain;
extern crate bytes;
extern crate futures;
extern crate tokio_io;
extern crate tokio_core;
extern crate hyper;
extern crate maud;

#[macro_use] mod utils;
pub mod error;
mod pages;

use std::io;
use futures::{ Future, Stream };
use futures::future::{ self, FutureResult };
use tokio_core::reactor::Handle;
use hyper::{ Get, Post, StatusCode, Body };
use hyper::header::ContentLength;
use hyper::server::{ Service, Request, Response };


#[derive(Debug, Clone)]
pub struct Httpd {
    handle: Handle,
    debug_flag: bool
}


impl Service for Httpd {
    type Request = Request;
    type Response = Response;
    type Error = hyper::Error;
    type Future = FutureResult<Self::Response, Self::Error>;

    fn call(&self, req: Request) -> Self::Future {
        match pages::process(self, &req) {
            Ok(_) => unimplemented!(),
            Err(err) => future::ok(match err.kind() {
                io::ErrorKind::NotFound =>
                    pages::fail(StatusCode::NotFound, None),
                io::ErrorKind::PermissionDenied =>
                    pages::fail(StatusCode::Forbidden, None),
                _ => pages::fail(
                    StatusCode::InternalServerError,
                    if self.debug_flag { Some(err) } else { None }
                )
            })
        }
    }
}

impl Httpd {
    pub fn new(handle: &Handle) -> Self {
        Httpd {
            handle: handle.clone(),
            debug_flag: true
        }
    }

    pub fn with_debug_flag(mut self, debug_flag: bool) -> Self {
        self.debug_flag = debug_flag;
        self
    }
}
