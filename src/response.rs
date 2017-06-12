use std::{ io, fmt };
use hyper::{ header, StatusCode, Response, Body };
use slog::Logger;


pub const BOUNDARY: &str = boundary!();

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
pub fn not_modified(log: &Logger, display: fmt::Arguments) -> Response {
    debug!(log, "cache"; "tag" => display);

    Response::new()
        .with_status(StatusCode::NotModified)
        .with_body(Body::empty())
}
