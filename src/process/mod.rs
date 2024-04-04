mod entity;
mod sortdir;

use std::io;
use std::ops::Range;
use std::path::PathBuf;
use std::fs::{ Metadata, ReadDir };
use log::*;
use futures::future::TryFutureExt;
use bytes::Bytes;
use hyper::{ Request, Response, Method, StatusCode };
use hyper::body::Incoming;
use http::HeaderMap;
use headers::HeaderMapExt;
use if_chain::if_chain;
use maud::Render;
use crate::WebDir;
use crate::file::File;
use crate::body::ResponseBody as Body;
use crate::common::{ path_canonicalize, decode_path, html_utf8, LimitFile };
use self::entity::Entity;
use self::sortdir::{ up, SortDir };


pub struct Process<'a> {
    webdir: &'a WebDir,
    req: Request<Incoming>
}

impl<'a> Process<'a> {
    pub fn new(webdir: &'a WebDir, req: Request<Incoming>) -> Process<'a> {
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

        let (mut sender, body) = Body::channel(None);

        debug!("send/dir: {}", is_top);

        let fut = async move {
            sender.send_data(Bytes::from_static(HTML_HEADER.as_bytes())).await?;
            sender.send_data(Bytes::from(up(is_top).into_string().into_bytes())).await?;
            for entry in SortDir::new(dir) {
                let string = entry?.render().into_string();
                sender.send_data(Bytes::from(string.into_bytes())).await?;
            }
            sender.send_data(Bytes::from_static(HTML_FOOTER.as_bytes())).await?;

            Ok(()) as anyhow::Result<()>
        }.unwrap_or_else(|err| error!("send/dir: {:?}", err));

        tokio::spawn(fut);

        let mut resp = Response::new(body);
        *resp.status_mut() = StatusCode::OK;
        resp.headers_mut()
            .typed_insert(html_utf8());
        resp
    }

    fn process_file(self, path: PathBuf, metadata: Metadata) -> Response<Body> {
        let entity = Entity::new(&path, &metadata);

        let entity::Result(status, mut map, value) = entity.result(self.req.headers());
        let mut resp = match value {
            entity::Value::Error(err) => {
                map.typed_insert(html_utf8());
                Response::new(Body::one(err))
            },
            entity::Value::None => Response::new(self.sendchunk(&entity, None)),
            entity::Value::Range(range) => Response::new(self.sendchunk(&entity, Some(range))),
            entity::Value::Multipart(boundary, ranges) => {
                if Method::HEAD == self.req.method() {
                    return Response::new(Body::empty());
                }

                let mime_type = mime_guess::from_path(entity.path).first_or_octet_stream();
                let boundary1 = format!("--{}\r\n", boundary);
                let boundary2 = format!("--{}--", boundary);

                let path = entity.path.to_owned();
                let length = entity.length;
                let (mut sender, body) = Body::channel(None);

                let fut = async move {
                    let mut fd = File::open(&path).await?;

                    for range in ranges {
                        let mut map = HeaderMap::new();
                        map.typed_insert(headers::ContentType::from(mime_type.clone()));
                        map.typed_insert(headers::ContentRange::bytes(range.clone(), length).unwrap());

                        let mut headers = boundary1.as_bytes().to_vec();
                        for (name, val) in &map {
                            headers.extend_from_slice(name.as_str().as_bytes());
                            headers.extend_from_slice(b": ");
                            headers.extend_from_slice(val.as_bytes());
                            headers.extend_from_slice(b"\r\n");
                        }
                        headers.extend_from_slice(b"\r\n");

                        debug!("send/multipart: {:?}", range);

                        let start = range.start;
                        let len = range.end - range.start;
                        fd.seek(io::SeekFrom::Start(start)).await?;
                        let mut fd = LimitFile::new(&mut fd, len);

                        sender.send_data(Bytes::from(headers)).await?;
                        while let Some(buf) = fd.next_chunk().await? {
                            sender.send_data(buf).await?;
                        }
                        sender.send_data(Bytes::from_static(b"\r\n")).await?;
                    }

                    sender.send_data(Bytes::from(boundary2)).await?;

                    Ok(()) as anyhow::Result<()>
                }.unwrap_or_else(|err| error!("send/multipart: {:?}", err));

                tokio::spawn(fut);
                Response::new(body)
            }
        };

        *resp.status_mut() = status;
        *resp.headers_mut() = map;
        resp
    }

    pub fn sendchunk(&self, entity: &Entity, range: Option<Range<u64>>) -> Body {
        if Method::HEAD == self.req.method() {
            return Body::empty();
        }

        debug!("send/chunk: {:?}", range);

        let path = entity.path.to_owned();
        let range = range.unwrap_or(0..entity.length);
        let start = range.start;
        let len = range.end - range.start;
        let (mut sender, body) = Body::channel(Some(len as u64));

        let fut = async move {
            let mut fd = {
                let mut fd = File::open(&path).await?;
                fd.seek(io::SeekFrom::Start(start)).await?;
                fd
            };
            let mut fd = LimitFile::new(&mut fd, len);

            while let Some(buf) = fd.next_chunk().await? {
                sender.send_data(buf).await?;
            }

            Ok(()) as anyhow::Result<()>
        }.unwrap_or_else(|err| error!("send/chunk: {:?}", err));

        tokio::spawn(fut);
        body
    }
}
