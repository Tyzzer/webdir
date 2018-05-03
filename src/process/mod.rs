mod sortdir;
mod entity;

use std::io;
use std::ops::Range;
use std::path::{ Path, PathBuf };
use std::fs::{ Metadata, ReadDir };
use futures::{ stream, Stream, Future };
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
    is_root: bool,
    path: PathBuf
}

impl<'a> Process<'a> {
    #[inline]
    pub fn new(httpd: &'a Httpd, log: &'a Logger, req: &'a Request) -> Process<'a> {
        let path_buf = decode_path(req.path());
        let (depth, path) = path_canonicalize(&httpd.root, path_buf);
        Process { httpd, log, req, path, is_root: depth == 0 }
    }

    #[inline]
    pub fn process(&self) -> io::Result<Response> {
        let metadata = self.path.metadata()?;

        if let Ok(dir) = self.path.read_dir() {
            let index_path = self.path.join("index.html");
            if_chain!{
                if self.httpd.index;
                if let Ok(try_index) = index_path.metadata();
                if try_index.is_file();
                then {
                    self.process_file(&index_path, &try_index)
                } else {
                    self.process_dir(dir)
                }
            }
        } else {
            self.process_file(&self.path, &metadata)
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
        const HTML_FOOTER: &str = "</tbody></table></body></html>";

        let mut res = Response::new();

        if self.req.method() == &Head {
            res.set_body(Body::empty());
        } else {
            let log = self.log.clone();
            let (send, body) = Body::pair();
            res.set_body(body);

            debug!(self.log, "process"; "send" => "senddir");

            let done = stream::once::<_, error::Error>(Ok(chunk!(HTML_HEADER)))
                .chain(stream::once(Ok(chunk!(into up(self.is_root)))))
                .chain(stream::iter_ok(SortDir::new(dir))
                    .map(|p| p.and_then(|m| chunk!(into m.render())).map_err(Into::into))
                )
                .chain(stream::once(Ok(chunk!(HTML_FOOTER))))
                .map_err(error::Error::from)
                .forward(send)
                .map(drop)
                .map_err(move |err| error!(log, "send"; "err" => format_args!("{}", err)));

            self.httpd.remote.spawn(done);
        }

        // TODO https://github.com/hyperium/mime/issues/52
        let mime = "text/html; charset=utf-8".parse().unwrap();
        Ok(res.with_header(header::ContentType(mime)))
    }

    fn process_file(&self, path: &Path, metadata: &Metadata) -> io::Result<Response> {
        let entity = Entity::new(path, metadata, self.log, self.httpd.chunk_length);

        match entity.check(self.req.headers()) {
            EntifyResult::Err(resp) => Ok(resp.with_headers(entity.headers(false))),
            EntifyResult::None => {
                let fd = entity.open()?;
                self.send(fd, None)
                    .map(|res| res
                        .with_headers(entity.headers(false))
                        .with_header(header::ContentLength(entity.length))
                    )
            },
            EntifyResult::One(range) => {
                debug!(self.log, "process"; "range" => format_args!("{:?}", range));

                let fd = entity.open()?;
                self.send(fd, Some(range.clone()))
                    .map(|res| res
                         .with_status(StatusCode::PartialContent)
                         .with_headers(entity.headers(false))
                         .with_header(header::ContentLength(range.end - range.start))
                         .with_header(header::ContentRange(header::ContentRangeSpec::Bytes {
                            range: Some((range.start, range.end - 1)), instance_length: Some(entity.length)
                        }))
                    )
            },
            EntifyResult::Vec(ranges) => {
                const BOUNDARY_LINE: &str = concat!("--", boundary!(), "\r\n");

                debug!(self.log, "process"; "ranges" => format_args!("{:?}", ranges));

                let mut res = Response::new();

                if self.req.method() == &Head {
                    return Ok(res
                        .with_status(StatusCode::PartialContent)
                        .with_headers(entity.headers(true))
                        .with_body(Body::empty())
                    );
                }

                let log = self.log.clone();
                let (send, body) = Body::pair();
                res.set_body(body);

                let fd = entity.open()?;
                let mime_type = guess_mime_type(path);

                let done = stream::iter_ok::<_, error::Error>(ranges.into_iter())
                    .and_then(move |range| {
                        let length = range.end - range.start;
                        let mut headers = header::Headers::new();
                        headers.set(header::ContentType(mime_type.clone()));
                        headers.set(header::ContentRange(header::ContentRangeSpec::Bytes {
                            range: Some((range.start, range.end - 1)), instance_length: Some(length)
                        }));

                        fd.try_clone()?
                            .read(range)
                            .map(|fut| stream::once(Ok(chunk!(BOUNDARY_LINE)))
                                .chain(stream::once(Ok(chunk!(format!("{}\r\n", headers)))))
                                .chain(fut)
                                .chain(stream::once(Ok(chunk!("\r\n"))))
                            )
                            .map_err(Into::into)
                    })
                    .flatten()
                    .forward(send)
                    .map(drop)
                    .map_err(move |err| error!(log, "send"; "err" => format_args!("{}", err)));

                self.httpd.remote.spawn(done);

                Ok(res
                    .with_status(StatusCode::PartialContent)
                    .with_headers(entity.headers(true))
                )
            }
        }
    }

    #[cfg(not(feature = "sendfile"))]
    fn send(&self, fd: file::File, range: Option<Range<u64>>) -> io::Result<Response> {
        let mut res = Response::new();

        if self.req.method() == &Head {
            res.set_body(Body::empty());
        } else {
            let range = range.unwrap_or_else(|| 0..fd.length);

            let log = self.log.clone();
            let (send, body) = Body::pair();
            res.set_body(body);

            debug!(self.log, "process"; "send" => "readchunk");

            let done = fd.read(range)?
                .forward(send)
                .map(drop)
                .map_err(move |err| error!(log, "send"; "err" => format_args!("{}", err)));

            self.httpd.remote.spawn(done);
        }

        Ok(res)
    }

    #[cfg(feature = "sendfile")]
    fn send(&self, fd: file::File, range: Option<Range<u64>>) -> io::Result<Response> {
        use futures::future;

        let mut res = Response::new();
        let range = range.unwrap_or_else(|| 0..fd.length);

        if self.req.method() == &Head {
            res.set_body(Body::empty());
        } else if let &Some(ref socket) = &self.httpd.socket {
            let log = self.log.clone();
            res.set_body(Body::empty());

            debug!(self.log, "process"; "send" => "sendfile");

            let done = fd.sendfile(range, socket.clone())?
                .for_each(|_| future::ok(()))
                .map_err(move |err| error!(log, "send"; "err" => format_args!("{}", err)));

            self.httpd.remote.spawn(done);
        } else {
            let log = self.log.clone();
            let (send, body) = Body::pair();
            res.set_body(body);

            debug!(self.log, "process"; "send" => "readchunk");

            let done = fd.read(range)?
                .forward(send)
                .map(drop)
                .map_err(move |err| error!(log, "send"; "err" => format_args!("{}", err)));

            self.httpd.remote.spawn(done);
        }

        Ok(res)
    }
}
