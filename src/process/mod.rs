use std::io;
use std::path::PathBuf;
use std::fs::{ fs, Metadata };
use failure::Fallible;
use tokio::prelude::*;
use tokio::net::TcpStream;
use hyper::{ Request, Response, Body };
use if_chain::if_chain;
use crate::stream::Stream as WebStream;
use crate::common::{ path_canonicalize, decode_path };
use crate::WebDir;


pub struct Process<'a> {
    webdir: &'a WebDir,
    req: Request<Body>
}

impl<'a> Process<'a> {
    pub fn new(webdir: &'a WebDir, req: Request<Body>) -> Process<'a> {
        Process { webdir, req }
    }

    pub fn process(self) -> Fallible<Response<Body>> {
        let path = decode_path(self.req.uri().path());
        let (depth, target) =
            path_canonicalize(&self.webdir.root, &path);
        let metadata = target.metadata()?;

        if let Ok(dir) = target.read_dir() {
            if_chain!{
                if self.webdir.index;
                if let index_path = target.join("index.html");
                if let Ok(try_index) = index_path.metadata();
                if try_index.is_file();
                then {
                    self.process_file(index_path, try_index)
                } else {
                    unimplemented!()
                }
            }
        } else {
            self.process_file(target, metadata)
        }
    }

    fn process_file(self, path: PathBuf, metadata: Metadata) -> Fallible<Response<Body>> {
        unimplemented!()
    }
}
