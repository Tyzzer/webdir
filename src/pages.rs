use std::io;
use std::sync::Arc;
use std::fs::File;
use std::os::unix::ffi::OsStringExt;
use std::ffi::OsString;
use url::percent_encoding;
use futures::{ stream, Stream, Future, Sink };
use hyper::{ header, StatusCode, Body, Chunk };
use hyper::server::{ Request, Response };
use maud::Render;
use ::utils::path_canonicalize;
use ::render::{ Entry, up };
use ::{ error, Httpd };


const HTML_HEADER: &str = "<html><head><style>\
    .time { padding-left: 12em; }\
    .size {\
        float: right;\
        padding-left: 2em;\
    }\
</style></head><body><table><tbody>";
const HTML_FOOTER: &str = "</tbody></table</body></html>";


pub fn process(httpd: &Httpd, req: &Request) -> io::Result<Response> {
    let path_buf = percent_encoding::percent_decode(req.path().as_bytes())
        .collect::<Vec<u8>>();
    let path_buf = OsString::from_vec(path_buf);
    let (depth, target_path) = path_canonicalize(&httpd.root, path_buf);
    let metadata = target_path.metadata()?;
    let res = Response::new();

    if let Ok(dirs) = target_path.read_dir() {
        let (send, body) = Body::pair();
        let arc_root = Arc::clone(&httpd.root);

        let done = send.send(Ok(HTML_HEADER.into()))
            .and_then(move |send| send
                .send(Ok(Chunk::from(up(depth == 0).into_string())))
            )
            .map_err(error::Error::from)
            .and_then(|send| stream::iter(dirs)
                .map(move |p| Entry::new(&arc_root, p)
                     .map(|m| Chunk::from(m.render().into_string()))
                     .map_err(Into::into)
                )
                .map_err(Into::into)
                .forward(send)
            )
            .and_then(|(_, send)| send
                .send(Ok(HTML_FOOTER.into()))
                .map_err(Into::into)
            )
            .map(|_| ())
            .map_err(|_| ());

        httpd.handle.spawn(done);

        Ok(res
            .with_header(header::ContentType::html())
            .with_body(body)
        )
    } else if let Ok(_) = File::open(&target_path) {
        unimplemented!()
    } else {
        unimplemented!()
    }
}

#[inline]
pub fn fail(status: StatusCode, err: Option<io::Error>) -> Response {
    Response::new()
        .with_status(status)
        .with_header(header::ContentType::html())
        .with_body({
            html!{
                h1 strong "( ・_・)"
                @if let Some(err) = err {
                    h2 Debug(err)
                }
            }
        }.into_string())
}
