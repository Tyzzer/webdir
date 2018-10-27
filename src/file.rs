use std::cmp;
use std::io::{ self, SeekFrom };
use std::ops::Range;
use tokio::prelude::*;
use tokio::fs as tfs;
use tokio_io::io::Window;
use hyper::body::Sender;
use crate::common::econv;

pub const CHUNK_LENGTH: usize = 1 << 16;


pub struct TryClone(pub tfs::File);

impl Stream for TryClone {
    type Item = tfs::File;
    type Error = io::Error;

    fn poll(&mut self) -> Poll<Option<Self::Item>, Self::Error> {
        let fd = try_ready!(self.0.poll_try_clone());
        Ok(Async::Ready(Some(fd)))
    }
}

pub struct ChunkReader {
    fd: tfs::File,
    range: Range<u64>,
    buf: Vec<u8>
}

impl ChunkReader {
    pub fn new(fd: tfs::File, range: Range<u64>) -> Self {
        ChunkReader { fd, range, buf: vec![0; CHUNK_LENGTH] }
    }
}

impl Stream for ChunkReader {
    type Item = hyper::Chunk;
    type Error = io::Error;

    fn poll(&mut self) -> Poll<Option<Self::Item>, Self::Error> {
        let want_len = cmp::min((self.range.end - self.range.start) as _, self.buf.len());

        if want_len > 0 {
            let mut window = Window::new(&mut self.buf[..]);
            window.set_end(want_len);

            try_ready!(self.fd.poll_seek(SeekFrom::Start(self.range.start)));
            let read_len = try_ready!(self.fd.poll_read(window.as_mut()));

            self.range.start += read_len as u64;
            window.set_end(read_len);

            let chunk = Vec::from(window.as_ref());
            Ok(Async::Ready(Some(chunk.into())))
        } else {
            Ok(Async::Ready(None))
        }
    }
}

pub struct SenderSink(pub Sender);

impl Sink for SenderSink {
    type SinkItem = hyper::Chunk;
    type SinkError = io::Error;

    fn start_send(&mut self, item: Self::SinkItem) -> Result<AsyncSink<Self::SinkItem>, Self::SinkError> {
        match self.0.poll_ready() {
            Ok(Async::Ready(())) => (),
            Ok(Async::NotReady) => return Ok(AsyncSink::NotReady(item)),
            Err(e) => return Err(econv(e))
        }

        match self.0.send_data(item) {
            Ok(()) => Ok(AsyncSink::Ready),
            Err(item) => Ok(AsyncSink::NotReady(item)),
        }
    }

    fn poll_complete(&mut self) -> Poll<(), Self::SinkError> {
        Ok(Async::Ready(()))
    }
}
