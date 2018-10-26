mod entity;

use std::io;
use std::ops::Range;
use std::path::PathBuf;
use std::fs::{ self, Metadata };
use log::{ log, error };
use failure::Fallible;
use tokio::prelude::*;
use tokio::fs as tfs;
use tokio::net::TcpStream;
use hyper::{ Request, Response, Body };
use headers_core::HeaderMapExt;
use headers_core::header::HeaderMap;
use headers_ext as header;
use mime_guess::guess_mime_type;
use if_chain::if_chain;
use crate::WebDir;
use crate::file::{ ChunkReader, SenderSink, TryClone };
use crate::common::{ path_canonicalize, decode_path };
use self::entity::Entity;


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
        let entity = Entity::new(&path, &metadata);

        match entity.result(self.req.headers()) {
            entity::Result(status, map, entity::Value::Err(err)) => {
                let body = Body::from(format!("Error: {:#?}", err));
                let mut resp = Response::new(body);
                *resp.status_mut() = status;
                *resp.headers_mut() = map;
                Ok(resp)
            },
            entity::Result(status, map, entity::Value::None) => {
                let body = self.sendchunk(&entity, None);
                let mut resp = Response::new(body);
                *resp.status_mut() = status;
                *resp.headers_mut() = map;
                Ok(resp)
            },
            entity::Result(status, map, entity::Value::One(range)) => {
                let body = self.sendchunk(&entity, Some(range.clone()));
                let mut resp = Response::new(body);
                *resp.status_mut() = status;
                *resp.headers_mut() = map;
                Ok(resp)
            },
            entity::Result(status, map, entity::Value::Vec(boundary, ranges)) => {
                let mime_type = guess_mime_type(entity.path);
                let boundary1 = format!("--{}\r\n", boundary);
                let boundary2 = format!("--{}--", boundary);

                let (sender, body) = Body::channel();
                let sink = SenderSink(sender);
                let length = entity.length;

                let done = tfs::File::open(entity.path.to_owned())
                    .and_then(move |fd| stream::iter_ok(ranges.into_iter())
                        .zip(TryClone(fd))
                        .map(move |(range, fd)| {
                            let mut map = HeaderMap::new();
                            map.typed_insert(header::ContentType::from(mime_type.clone()));
                            map.typed_insert(header::ContentRange::bytes(
                                range.start,
                                range.end - 1,
                                length
                            ));

                            let mut headers = boundary1.as_bytes().to_vec();
                            for (name, val) in &map {
                                headers.extend_from_slice(name.as_str().as_bytes());
                                headers.extend_from_slice(b": ");
                                headers.extend_from_slice(val.as_bytes());
                                headers.extend_from_slice(b"\r\n");
                            }
                            headers.extend_from_slice(b"\r\n");

                            chain!{
                                type Item = _;
                                type Error = _;

                                ( headers ),
                                ( + ChunkReader::new(fd, range) )
                            }
                        })
                        .flatten()
                        .chain(stream::once(Ok(hyper::Chunk::from(boundary2))))
                        .forward(sink)
                    )
                    .map(drop)
                    .map_err(|err| error!("send/multipart: {:?}", err));

                hyper::rt::spawn(done);

                let mut resp = Response::new(body);
                *resp.status_mut() = status;
                *resp.headers_mut() = map;
                Ok(resp)
            }
        }
    }

    pub fn sendchunk(&self, entity: &Entity, range: Option<Range<u64>>) -> Body {
        let range = range.unwrap_or(0..entity.length);
        let (sender, body) = Body::channel();
        let sink = SenderSink(sender);

        let done = tfs::File::open(entity.path.to_owned())
            .map(|fd| ChunkReader::new(fd, range))
            .and_then(|reader| reader.forward(sink))
            .map(drop)
            .map_err(|err| error!("send/chunk: {:?}", err));

        hyper::rt::spawn(done);

        body
    }
}
