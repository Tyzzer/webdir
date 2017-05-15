use std::io;
use std::sync::Arc;
use std::fs::File;
use std::os::unix::ffi::OsStringExt;
use std::ffi::OsString;
use url::percent_encoding;
use futures::{ stream, Stream, Future, Sink };
use hyper::{ header, StatusCode, Body };
use hyper::server::{ Request, Response };
use maud::Render;
use slog::Logger;
use ::utils::path_canonicalize;
use ::render::up;
use ::sortdir::SortDir;
use ::{ error, Httpd };


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
    let res = Response::new();

    if let Ok(dirs) = target_path.read_dir() {
        let (send, body) = Body::pair();
        let arc_root = Arc::clone(&httpd.root);

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

        httpd.handle.spawn(done);

        Ok(res
            .with_header(header::ContentType::html())
            .with_body(body)
        )
    } else {
        let _ = File::open(&target_path)?;

        unimplemented!()
    }
}

#[inline]
pub fn fail(log: &Logger, status: StatusCode, err: io::Error) -> Response {
    debug!(log, "fail"; "err" => format_args!("{}", err));

    Response::new()
        .with_status(status)
        .with_header(header::ContentType::html())
        .with_body({
            html!{
                h1 strong "( ・_・)"
                h2 (err)
            }
        }.into_string())
}
