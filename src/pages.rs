use std::io;
use std::fmt::{ self, Write };
use std::fs::File;
use maud::{Render, Escaper};
use hyper::StatusCode;
use hyper::server::{ Request, Response };
use ::utils::path_canonicalize;
use ::Httpd;


/// Renders the given value using its `Debug` implementation.
struct Debug<T: fmt::Debug>(T);

impl<T: fmt::Debug> Render for Debug<T> {
    fn render_to(&self, output: &mut String) {
        let mut escaper = Escaper::new(output);
        write!(escaper, "{:?}", self.0).unwrap();
    }
}

pub fn process(httpd: &Httpd, req: &Request) -> io::Result<()> {
    let target_path = path_canonicalize(req.path());
    let metadata = target_path.metadata()?;

    if let Ok(dir) = target_path.read_dir() {
        unimplemented!()
    } else if let Ok(fd) = File::open(&target_path) {
        unimplemented!()
    } else {
        unimplemented!()
    }
}

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
