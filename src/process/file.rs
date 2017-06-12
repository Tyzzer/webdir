use std::{ fs, cmp };
use std::ops::Range;
use std::io::{ self, Read, Seek, SeekFrom };
use futures::{ Stream, Poll, Async };
use tokio_io::io::Window;
use tokio_core::reactor::Handle;
use hyper;
use ::error;


pub const CHUNK_BUFF_LENGTH: usize = 4096;

pub struct File {
    fd: fs::File
}

impl File {
    pub fn new(fd: fs::File, _handle: Handle) -> io::Result<Self> {
        Ok(File { fd })
    }

    pub fn read(&self, range: Range<u64>) -> io::Result<ReadFut> {
        let fd = self.fd.try_clone()?;
        let buf = Box::new([0; CHUNK_BUFF_LENGTH]);

        Ok(ReadFut { fd, range, buf })
    }
}

pub struct ReadFut {
    fd: fs::File,
    range: Range<u64>,
    buf: Box<[u8; CHUNK_BUFF_LENGTH]>
}

impl Stream for ReadFut {
    type Item = hyper::Result<hyper::Chunk>;
    type Error = error::Error;

    fn poll(&mut self) -> Poll<Option<Self::Item>, Self::Error> {
        let read_len = cmp::min((self.range.end - self.range.start) as _, CHUNK_BUFF_LENGTH);

        if read_len > 0 {
            let mut window = Window::new(&mut self.buf[..]);
            window.set_end(read_len);
            self.fd.seek(SeekFrom::Start(self.range.start as _))?;

            let read_len = self.fd.read(window.as_mut())?;
            self.range.start += read_len as _;

            window.set_end(read_len);
            let chunk = Vec::from(window.as_ref());
            let chunk = hyper::Chunk::from(chunk);
            Ok(Async::Ready(Some(Ok(chunk))))
        } else {
            Ok(Async::Ready(None))
        }
    }
}
