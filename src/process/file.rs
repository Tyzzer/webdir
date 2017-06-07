use std::ops::Range;
use futures::{ Stream, Poll };
use hyper;
use ::error;


#[derive(Clone)]
pub struct File;

impl File {
    pub fn read(&self, range: Range<u64>) -> ReadFut {
        ReadFut { fd: self.clone(), range }
    }
}

pub struct ReadFut {
    fd: File,
    range: Range<u64>,
}

impl Stream for ReadFut {
    type Item = hyper::Result<hyper::Chunk>;
    type Error = error::Error;

    fn poll(&mut self) -> Poll<Option<Self::Item>, Self::Error> {
        unimplemented!()
    }
}
