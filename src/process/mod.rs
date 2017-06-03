mod sortdir;
mod entity;
mod multipart;

use std::io;
use std::ffi::OsString;
use std::path::PathBuf;
use std::fs::{ Metadata, ReadDir };
use std::os::unix::ffi::OsStringExt;
use futures::{ stream, Stream, Future, Sink };
use hyper::{ header, Request, Response, Head, Body };
use url::percent_encoding;
use maud::Render;
use slog::Logger;
use ::utils::path_canonicalize;
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
        let path_buf = percent_encoding::percent_decode(req.path().as_bytes())
            .collect::<Vec<u8>>();
        let path_buf = OsString::from_vec(path_buf);
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

        if self.req.method() != &Head {
            let log = self.log.clone();
            let is_root = self.depth == 0;
            let root = self.httpd.root.clone();
            let (send, body) = Body::pair();
            res.set_body(body);

            let done = send.send(Ok(HTML_HEADER.into()))
                .and_then(move |send| send.send(chunk!(ok up(is_root))))
                .map_err(error::Error::from)
                .and_then(|send| stream::iter(SortDir::new(root, dir))
                    .map(|p| p.map(|m| chunk!(m.render())).map_err(Into::into))
                    .map_err(Into::into)
                    .forward(send)
                )
                .and_then(|(_, send)| send
                    .send(Ok(HTML_FOOTER.into()))
                    .map_err(Into::into)
                )
                .map(|_| ())
                .map_err(move |err| error!(log, "error"; "err" => format_args!("{}", err)));

            self.httpd.handle.spawn(move |_| done);
        }

        Ok(res.with_header(header::ContentType::html()))
    }

    fn process_file(&self, metadata: &Metadata) -> io::Result<Response> {
        let entity = Entity::new(&self.path, metadata, self.log);

        match entity.check(self.req.headers()) {
            EntifyResult::Err(resp) => Ok(resp),
            EntifyResult::None => unimplemented!(),
            EntifyResult::One(range) => unimplemented!(),
            EntifyResult::Vec(ranges) => unimplemented!()
        }
    }
}
