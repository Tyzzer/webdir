use std::{ io, fmt };
use std::sync::Arc;
use std::path::Path;
use std::fs::{ File, Metadata };
use std::os::unix::ffi::OsStringExt;
use std::ffi::OsString;
use futures::{ stream, Stream, Future, Sink };
use hyper::{ header, StatusCode, Head, Body };
use hyper::server::{ Request, Response };
use url::percent_encoding;
use maud::Render;
use slog::Logger;
use ::utils::path_canonicalize;
use ::render::up;
use ::sortdir::SortDir;
use ::{ error, entity, Httpd };


const HTML_HEADER: &str = "<html><head><style>\
    .time { padding-left: 12em; }\
    .size {\
        float: right;\
        padding-left: 2em;\
    }\
</style></head><body><table><tbody>";
const HTML_FOOTER: &str = "</tbody></table</body></html>";


pub fn process(httpd: &Httpd, log: &Logger, req: &Request) -> io::Result<Response> {
    let path_buf = percent_encoding::percent_decode(req.path().as_bytes())
        .collect::<Vec<u8>>();
    let path_buf = OsString::from_vec(path_buf);
    let (depth, target_path) = path_canonicalize(&httpd.root, path_buf);

    let metadata = target_path.metadata()?;
    let log = log.clone();

    if let Ok(dirs) = target_path.read_dir() {
        let mut res = Response::new();

        if req.method() != &Head {
            let arc_root = Arc::clone(&httpd.root);
            let (send, body) = Body::pair();
            res.set_body(body);

            let done = send.send(Ok(HTML_HEADER.into()))
                .and_then(move |send| send.send(chunk!(ok up(depth == 0))))
                .map_err(error::Error::from)
                .and_then(|send| stream::iter(SortDir::new(arc_root, dirs))
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

            httpd.handle.spawn(move |_| done);
        }

        Ok(res.with_header(header::ContentType::html()))
    } else {
        let etag = entity::etag(&metadata);

        if let Some(&header::IfMatch::Items(ref etags)) = req.headers().get::<header::IfMatch>() {
            if etags.iter().find(|e| etag.strong_eq(e)).is_none() {
                return Ok(fail(&log, false, StatusCode::PreconditionFailed, &err!(Other, "Precondition failed")));
            }
        }

        if let Some(&header::IfNoneMatch::Items(ref etags)) = req.headers().get::<header::IfNoneMatch>() {
            if etags.iter().find(|e| etag.weak_eq(e)).is_some() {
                return Ok(not_modified(&log, &target_path, &metadata, format_args!("{}", etag)));
            }
        }

        if let Some(&header::IfModifiedSince(ref date)) = req.headers().get::<header::IfModifiedSince>() {
            if let Ok(ndate) = metadata.modified() {
                if date >= &header::HttpDate::from(ndate) {
                    return Ok(not_modified(&log, &target_path, &metadata, format_args!("{}", date)));
                }
            }
        }

        let mut range = req.headers().get::<header::Range>();
        match req.headers().get::<header::IfRange>() {
            Some(&header::IfRange::EntityTag(ref e)) => if !etag.strong_eq(e) {
                range = None;
            },
            Some(&header::IfRange::Date(ref date)) => if let Ok(ndate) = metadata.modified() {
                if date < &header::HttpDate::from(ndate) {
                    range = None;
                }
            },
            _ => ()
        }

        let _ = File::open(&target_path)?;

        if let Some(&header::Range::Bytes(ref ranges)) = req.headers().get::<header::Range>() {
            match entity::process_range(&log, ranges, metadata.len()) {
                Ok(ranges) => ranges,
                Err(res) => return Ok(res)
            };

            unimplemented!()
        } else {
            let (send, body) = Body::pair();
            let mut res = Response::new()
                .with_headers(entity::resp_headers(&target_path, &metadata))
                .with_header(header::ETag(etag))
                .with_body(body);

            if req.method() != &Head {
                unimplemented!()
            }

            Ok(res)
        }
    }
}

#[inline]
pub fn fail(log: &Logger, nobody: bool, status: StatusCode, err: &io::Error) -> Response {
    debug!(log, "fail"; "err" => format_args!("{}", err));

    let mut res = Response::new()
        .with_status(status)
        .with_header(header::ContentType::html());

    if nobody {
        res.set_body({
            html!{
                h1 strong "( ・_・)"
                h2 (err)
            }
        }.into_string());
    }

    res
}

#[inline]
pub fn not_modified(log: &Logger, path: &Path, metadata: &Metadata, display: fmt::Arguments) -> Response {
    debug!(log, "cache"; "tag" => display);

    Response::new()
        .with_status(StatusCode::NotModified)
        .with_headers(entity::resp_headers(&path, &metadata))
        .with_body(Body::empty())
}
