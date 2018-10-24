use std::{ fs, mem, cmp };
use std::io::{ self, SeekFrom };
use std::collections::VecDeque;
use std::ops::Range;
use failure::Fallible;
use tokio::prelude::*;
use tokio::fs as tfs;
use tokio::net::TcpStream;
use tokio_io::io::Window;
use hyper::body::{ Sender, Body };
use hyper::upgrade::Upgraded;
use tokio_linux_zio as zio;
use crate::stream::Stream as WebStream;
use crate::common::err;

const CHUNK_LENGTH: usize = 1 << 16;

macro_rules! try_ready {
    ($e:expr) => (match $e {
        Ok(tokio::prelude::Async::Ready(t)) => t,
        Ok(tokio::prelude::Async::NotReady) => return Ok(tokio::prelude::Async::NotReady),
        Err(e) => return Err(From::from(e)),
    })
}


pub struct ChunkStream {
    fd: tfs::File,
    range: Range<u64>,
    buf: Vec<u8>
}

pub struct SenderSink {
    sender: Sender
}

impl Stream for ChunkStream {
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

impl Sink for SenderSink {
    type SinkItem = hyper::Chunk;
    type SinkError = io::Error;

    fn start_send(&mut self, item: Self::SinkItem) -> Result<AsyncSink<Self::SinkItem>, Self::SinkError> {
        match self.sender.poll_ready() {
            Ok(Async::Ready(())) => (),
            Ok(Async::NotReady) => return Ok(AsyncSink::NotReady(item)),
            Err(e) => return Err(err(e))
        }

        match self.sender.send_data(item) {
            Ok(()) => Ok(AsyncSink::Ready),
            Err(item) => Ok(AsyncSink::NotReady(item)),
        }
    }

    fn poll_complete(&mut self) -> Poll<(), Self::SinkError> {
        Ok(Async::Ready(()))
    }
}
