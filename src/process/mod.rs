mod entity;
mod sortdir;

use std::io;
use std::ops::Range;
use std::path::PathBuf;
use std::fs::{ self, Metadata, ReadDir };
use log::{ log, error };
use tokio::prelude::*;
use tokio::fs as tfs;
use tokio::net::TcpStream;
use hyper::{ Request, Response, Body, StatusCode };
use headers_core::HeaderMapExt;
use headers_core::header::HeaderMap;
use headers_ext as header;
use mime_guess::guess_mime_type;
use if_chain::if_chain;
use maud::Render;
use crate::WebDir;
use crate::file::{ ChunkReader, SenderSink, TryClone };
use crate::common::{ path_canonicalize, decode_path, html_utf8 };
use self::entity::Entity;
use self::sortdir::{ up, SortDir };


pub struct Process<'a> {
    webdir: &'a WebDir,
    req: Request<Body>
}

impl<'a> Process<'a> {
    pub fn new(webdir: &'a WebDir, req: Request<Body>) -> Process<'a> {
        Process { webdir, req }
    }

    pub fn process(self) -> io::Result<Response<Body>> {
        let path = decode_path(self.req.uri().path());
        let (depth, target) =
            path_canonicalize(&self.webdir.root, &path);
        let metadata = target.metadata()?;

        Ok(if let Ok(dir) = target.read_dir() {
            if_chain!{
                if self.webdir.index;
                if let index_path = target.join("index.html");
                if let Ok(try_index) = index_path.metadata();
                if try_index.is_file();
                then {
                    self.process_file(index_path, try_index)
                } else {
                    self.process_dir(dir, depth == 0)
                }
            }
        } else {
            self.process_file(target, metadata)
        })
    }

    fn process_dir(self, dir: ReadDir, is_top: bool) -> Response<Body> {
        const HTML_HEADER: &str = "<html><head><style>\
            .time { padding-left: 12em; }\
            .size {\
                float: right;\
                padding-left: 2em;\
            }\
        </style></head><body><table><tbody>";
        const HTML_FOOTER: &str = "</tbody></table></body></html>";

        let (sender, body) = Body::channel();
        let sink = SenderSink(sender);

        let stream = chain! {
            type Item = _;
            type Error = io::Error;

            ( HTML_HEADER ),
            ( up(is_top).into_string() ),
            ( + stream::iter_result(SortDir::new(dir)
                    .map(|entry| entry
                        .map(|entry| entry.render().into_string())
                        .map(hyper::Chunk::from)
                    )
                )
            ),
            ( HTML_FOOTER )
        };

        let done = stream
            .forward(sink)
            .map(drop)
            .map_err(|err| error!("send/dir: {:?}", err));

        hyper::rt::spawn(done);

        let mut resp = Response::new(body);
        *resp.status_mut() = StatusCode::OK;
        resp.headers_mut()
            .typed_insert(html_utf8());
        resp
    }

    fn process_file(self, path: PathBuf, metadata: Metadata) -> Response<Body> {
        let entity = Entity::new(&path, &metadata);

        match entity.result(self.req.headers()) {
            entity::Result(status, mut map, entity::Value::Error(err)) => {
                let mut resp = Response::new(err);
                map.typed_insert(html_utf8());
                *resp.status_mut() = status;
                *resp.headers_mut() = map;
                resp
            },
            entity::Result(status, map, entity::Value::None) => {
                let body = self.sendchunk(&entity, None);
                let mut resp = Response::new(body);
                *resp.status_mut() = status;
                *resp.headers_mut() = map;
                resp
            },
            entity::Result(status, map, entity::Value::Range(range)) => {
                let body = self.sendchunk(&entity, Some(range.clone()));
                let mut resp = Response::new(body);
                *resp.status_mut() = status;
                *resp.headers_mut() = map;
                resp
            },
            entity::Result(status, map, entity::Value::Multipart(boundary, ranges)) => {
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
                resp
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
