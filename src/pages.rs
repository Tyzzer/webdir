use std::io;
use std::ops::Add;
use std::sync::Arc;
use std::borrow::Cow;
use std::fmt::{ self, Write };
use std::fs::{ File, DirEntry, Metadata };
use std::path::{ PathBuf, Path, StripPrefixError };
use std::ffi::OsString;
use std::os::unix::ffi::{ OsStringExt, OsStrExt };
use url::percent_encoding;
use futures::{ stream, Stream, Future };
use hyper::{ StatusCode, Body };
use hyper::server::{ Request, Response };
use chrono::Local;
use ::utils::path_canonicalize;
use ::{ error, Httpd };


struct Entry {
    metadata: Metadata,
    path: PathBuf
}

impl Entry {
    fn new(entry: DirEntry) -> io::Result<Self> {
        let metadata = entry.metadata()?;
        let path = entry.path();
        Ok(Entry { metadata, path })
    }

    fn name(&self) -> Cow<str> {
        self.path
            .file_name()
            .map(|p| p.to_string_lossy())
            .unwrap_or(Cow::Borrowed("../"))
    }

    fn uri(&self, base: &Path) -> Result<String, StripPrefixError> {
        self.path.strip_prefix(base)
            .map(|p| percent_encoding::percent_encode(
                p.as_os_str().as_bytes(),
                percent_encoding::PATH_SEGMENT_ENCODE_SET
            ))
            .map(|p| p.fold(String::new(), Add::add))
    }
}

pub fn process(httpd: &Httpd, req: &Request) -> io::Result<Response> {
    let path_buf = percent_encoding::percent_decode(req.path().as_bytes())
        .collect::<Vec<u8>>();
    let path_buf = OsString::from_vec(path_buf);
    let target_path = path_canonicalize(&httpd.root, path_buf);
    let metadata = target_path.metadata()?;
    let mut res = Response::new();

    if let Ok(dirs) = target_path.read_dir() {
        let (send, body) = Body::pair();
        let arc_root = Arc::clone(&httpd.root);

        let done = stream::iter(dirs)
            .and_then(Entry::new)
            .map(move |entry| html!{
                p {
                    @if let Ok(uri) = entry.uri(&arc_root) {
                        a href=(uri) (entry.name())
                    } @else {
                        (entry.name())
                    }
                }
            })
            .map(|m| Ok(m.into_string().into()))
            .map_err(error::Error::from)
            .forward(send)
            .map(|_| ())
            .map_err(|_| ());

        httpd.handle.spawn(done);

        Ok(res.with_body(body))
    } else if let Ok(fd) = File::open(&target_path) {
        unimplemented!()
    } else {
        unimplemented!()
    }
}

#[inline]
pub fn fail(status: StatusCode, err: Option<io::Error>) -> Response {
    Response::new()
        .with_status(status)
        .with_body({
            html!{
                h1 strong "( ・_・)"
                @if let Some(err) = err {
                    h2 Debug(err)
                }
            }
        }.into_string())
}
