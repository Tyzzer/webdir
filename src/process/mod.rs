use failure::Fallible;
use hyper::{ Request, Response, Body };
use crate::WebDir;


pub struct Process<'a> {
    webdir: &'a WebDir,
    req: &'a Request<Body>
}

impl<'a> Process<'a> {
    pub fn new(webdir: &'a WebDir, req: &'a Request<Body>) -> Process<'a> {
        Process { webdir, req }
    }

    pub fn process(&self) -> Fallible<Response<Body>> {
        let mut builder = Response::builder();
        builder.body(Body::empty())
            .map_err(Into::into)
    }
}
