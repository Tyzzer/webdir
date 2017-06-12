mod sortdir;
mod entity;
mod file;

use std::io;
use std::path::PathBuf;
use std::fs::{ Metadata, ReadDir };
use futures::{ stream, Stream, Future };
use hyper::{ header, Request, Response, Head, Body };
use maud::Render;
use slog::Logger;
use ::utils::{ path_canonicalize, decode_path };
use ::{ error, Httpd };
use self::sortdir::{ SortDir, up };
use self::entity::{ Entity, EntifyResult };


pub struct Process<'a> {
    httpd: &'a Httpd,
    log: &'a Logger,
    req: &'a Request,
    depth: usize,
    path: PathBuf
}

impl<'a> Process<'a> {
    pub fn new(httpd: &'a Httpd, log: &'a Logger, req: &'a Request) -> Process<'a> {
        let path_buf = decode_path(req.path());
        let (depth, path) = path_canonicalize(&httpd.root, path_buf);
        Process { httpd, log, req, depth, path }
    }

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

            let done = stream::once(Ok(Ok(HTML_HEADER.into())))
                .chain(stream::once(Ok(chunk!(ok up(is_root)))))
                .chain(stream::iter(SortDir::new(root, dir))
                    .map(|p| p.map(|m| chunk!(m.render())).map_err(Into::into))
                )
                .chain(stream::once(Ok(Ok(HTML_FOOTER.into()))))
                .map_err(error::Error::from)
                .forward(send)
                .map(drop)
                .map_err(move |err| error!(log, "error"; "err" => format_args!("{}", err)));

            self.httpd.remote.spawn(move |_| done);
        }

        Ok(res.with_header(header::ContentType::html()))
    }

    fn process_file(&self, metadata: &Metadata) -> io::Result<Response> {
        let entity = Entity::new(&self.path, metadata, self.log);

        match entity.check(self.req.headers()) {
            EntifyResult::Err(resp) => Ok(resp.with_headers(entity.headers(false))),
            ref result if self.req.method() == &Head => Ok(Response::new()
                .with_headers(entity.headers(
                    if let EntifyResult::Vec(_) = *result { true }
                    else { false }
                ))
                .with_body(Body::empty())
            ),
            EntifyResult::None => unimplemented!(),
            EntifyResult::One(range) => unimplemented!(),
            EntifyResult::Vec(ranges) => {
                let handle = self.httpd.remote.handle()
                    .ok_or_else(|| err!(Other, "Remote get handle fail"))?;

                let log = self.log.clone();
                let fd = entity.open(handle)?;
                let mut res = Response::new();
                let (send, body) = Body::pair();
                res.set_body(body);

                let done = stream::iter::<_, _, error::Error>(ranges.into_iter().map(Ok))
                    .map(move |range| fd.read(range))
                    .flatten()
                    .forward(send)
                    .map(drop)
                    .map_err(move |err| error!(log, "error"; "err" => format_args!("{}", err)));

                self.httpd.remote.spawn(move |_| done);

                Ok(res)
            }
        }
    }
}
