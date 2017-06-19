mod sortdir;
mod entity;

use std::io;
use std::ops::Range;
use std::path::PathBuf;
use std::rc::Rc;
use std::fs::{ Metadata, ReadDir };
use futures::{ future, stream, Stream, Future };
use hyper::{ header, Request, Response, Head, Body, StatusCode };
use mime_guess::guess_mime_type;
use maud::Render;
use slog::Logger;
use ::utils::{ path_canonicalize, decode_path };
use ::{ error, file, Httpd };
use self::sortdir::{ SortDir, up };
use self::entity::{ Entity, EntifyResult };


pub struct Process<'a> {
    httpd: &'a Httpd,
    log: &'a Logger,
    req: &'a Request,
    depth: usize,
    path: Rc<PathBuf>
}

impl<'a> Process<'a> {
    #[inline]
    pub fn new(httpd: &'a Httpd, log: &'a Logger, req: &'a Request) -> Process<'a> {
        let path_buf = decode_path(req.path());
        let (depth, path) = path_canonicalize(&httpd.root, path_buf);
        let path = Rc::new(path);
        Process { httpd, log, req, depth, path }
    }

    #[inline]
    pub fn process(&self) -> io::Result<Response> {
        let metadata = self.path.metadata()?;

        if let Ok(dir) = self.path.read_dir() {
            self.process_dir(dir)
        } else {
            self.process_file(&metadata)
        }
    }
}

impl<'a> Process<'a> {
    fn process_dir(&self, dir: ReadDir) -> io::Result<Response> {
        const HTML_HEADER: &str = "<html><head><style>\
            .time { padding-left: 12em; }\
            .size {\
                float: right;\
                padding-left: 2em;\
            }\
        </style></head><body><table><tbody>";
        const HTML_FOOTER: &str = "</tbody></table</body></html>";

        let mut res = Response::new();

        if self.req.method() == &Head {
            res.set_body(Body::empty());
        } else {
            let log = self.log.clone();
            let is_root = self.depth == 0;
            let root = self.httpd.root.clone();
            let (send, body) = Body::pair();
            res.set_body(body);

            debug!(self.log, "process"; "method" => "senddir");

            let done = stream::once(Ok(chunk!(HTML_HEADER)))
                .chain(stream::once(Ok(chunk!(into up(is_root)))))
                .chain(stream::iter(SortDir::new(root, dir))
                    .map(|p| p.and_then(|m| chunk!(into m.render())).map_err(Into::into))
                )
                .chain(stream::once(Ok(chunk!(HTML_FOOTER))))
                .map_err(error::Error::from)
                .forward(send)
                .map(drop)
                .map_err(move |err| debug!(log, "send"; "err" => format_args!("{}", err)));

            self.httpd.remote.spawn(move |_| done);
        }

        // TODO https://github.com/hyperium/mime/issues/52
        let mime = "text/html; charset=utf-8".parse().unwrap();
        Ok(res.with_header(header::ContentType(mime)))
    }

    fn process_file(&self, metadata: &Metadata) -> io::Result<Response> {
        let entity = Entity::new(self.path.clone(), metadata, self.log);

        match entity.check(self.req.headers()) {
            EntifyResult::Err(resp) => Ok(resp.with_headers(entity.headers(false))),
            EntifyResult::None => {
                let handle = self.httpd.remote.handle()
                    .ok_or_else(|| err!(Other, "Remote get handle fail"))?;
                let fd = entity.open(handle)?;
                self.send(&fd, None)
                    .map(|res| res
                        .with_headers(entity.headers(false))
                        .with_header(header::ContentLength(fd.len))
                    )
            },
            EntifyResult::One(range) => {
                debug!(self.log, "process"; "range" => format_args!("{:?}", range));
                let handle = self.httpd.remote.handle()
                    .ok_or_else(|| err!(Other, "Remote get handle fail"))?;
                let fd = entity.open(handle)?;
                self.send(&fd, Some(range.clone()))
                    .map(|res| res
                         .with_status(StatusCode::PartialContent)
                         .with_headers(entity.headers(false))
                         .with_header(header::ContentLength(range.end - range.start))
                         .with_header(header::ContentRange(header::ContentRangeSpec::Bytes {
                            range: Some((range.start, range.end - 1)), instance_length: Some(fd.len)
                        }))
                    )
            },
            EntifyResult::Vec(ranges) => {
                const BOUNDARY_LINE: &str = concat!("--", boundary!(), "\r\n");

                debug!(self.log, "process"; "ranges" => format_args!("{:?}", ranges));

                let handle = self.httpd.remote.handle()
                    .ok_or_else(|| err!(Other, "Remote get handle fail"))?;
                let mut res = Response::new();

                if self.req.method() == &Head {
                    return Ok(res
                        .with_status(StatusCode::PartialContent)
                        .with_headers(entity.headers(true))
                        .with_body(Body::empty())
                    );
                }

                let log = self.log.clone();
                let fd = entity.open(handle)?;
                let (send, body) = Body::pair();
                res.set_body(body);

                let content_type = header::ContentType(guess_mime_type(&*self.path));

                let done = stream::iter::<_, _, error::Error>(ranges.into_iter().map(Ok))
                    .and_then(move |range| {
                        let len = range.end - range.start;
                        let mut headers = header::Headers::new();
                        headers.set(content_type.clone());
                        headers.set(header::ContentRange(header::ContentRangeSpec::Bytes {
                            range: Some((range.start, range.end - 1)), instance_length: Some(len)
                        }));

                        fd.read(range)
                            .map(move |fut| {
                                stream::once(Ok(chunk!(BOUNDARY_LINE)))
                                    .chain(stream::once(Ok(chunk!(format!("{}\r\n", headers)))))
                                    .chain(fut)
                                    .chain(stream::once(Ok(chunk!("\r\n"))))
                            })
                            .map_err(Into::into)
                    })
                    .flatten()
                    .forward(send)
                    .map(drop)
                    .map_err(move |err| debug!(log, "send"; "err" => format_args!("{}", err)));

                self.httpd.remote.spawn(move |_| done);

                Ok(res
                    .with_status(StatusCode::PartialContent)
                    .with_headers(entity.headers(true))
                )
            }
        }
    }

    fn send(&self, fd: &file::File, range: Option<Range<u64>>) -> io::Result<Response> {
        use ::file::CHUNK_BUFF_LENGTH;
        let mut res = Response::new();

        if self.req.method() == &Head {
            res.set_body(Body::empty());
        } else if let (&Some(ref socket), true) = (&self.httpd.socket, CHUNK_BUFF_LENGTH >= fd.len as _) {
            let range = range.unwrap_or_else(|| 0..fd.len);
            let log = self.log.clone();
            res.set_body(Body::empty());

            debug!(self.log, "process"; "method" => "sendfile");

            let done = fd.sendfile(range, socket.clone())?
                //              ^^^ FIXME not work for keepalive
                .for_each(|_| future::empty())
                .map_err(move |err| debug!(log, "send"; "err" => format_args!("{}", err)));

            self.httpd.remote.spawn(move |_| done);
        } else {
            let range = range.unwrap_or_else(|| 0..fd.len);

            let log = self.log.clone();
            let (send, body) = Body::pair();
            res.set_body(body);

            debug!(self.log, "process"; "method" => "readchunk");

            let done = fd.read(range)?
                .forward(send)
                .map(drop)
                .map_err(move |err| debug!(log, "send"; "err" => format_args!("{}", err)));

            self.httpd.remote.spawn(move |_| done);
        }

        Ok(res)
    }
}
