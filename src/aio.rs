use std::{ io, fs, cmp };
use std::ops::Range;
use std::os::unix::io::AsRawFd;
use tokio::prelude::*;
use tokio_linux_aio::{ AioContext, AioReadResultFuture };
use crate::file::CHUNK_LENGTH;


pub struct HyperExecutor;

impl<F> future::Executor<F> for HyperExecutor
where F: Future<Item=(), Error=()> + Send + 'static
{
    fn execute(&self, fut: F) -> Result<(), future::ExecuteError<F>> {
        hyper::rt::spawn(fut);
        Ok(())
    }
}

pub struct AioReader {
    context: AioContext,
    fd: fs::File,
    range: Range<u64>,
    fut: Option<AioReadResultFuture<Vec<u8>>>
}

impl AioReader {
    pub fn new(context: AioContext, fd: fs::File, range: Range<u64>) -> Self {
        AioReader { context, fd, range, fut: None }
    }
}

impl Stream for AioReader {
    type Item = hyper::Chunk;
    type Error = io::Error;

    fn poll(&mut self) -> Poll<Option<Self::Item>, Self::Error> {
        if let Some(mut fut) = self.fut.take() {
            match fut.poll() {
                Ok(Async::Ready(buf)) => {
                    self.range.start += buf.len() as u64;
                    return Ok(Async::Ready(Some(buf.into())))
                },
                Ok(Async::NotReady) => {
                    self.fut = Some(fut);
                    return Ok(Async::NotReady);
                },
                Err(err) => return Err(err.error)
            }

        }

        let want_len = cmp::min((self.range.end - self.range.start) as _, CHUNK_LENGTH);

        if want_len > 0 {
            let buf = vec![0; want_len];

            let mut fut = self.context.read(self.fd.as_raw_fd(), self.range.start, buf);
            match fut.poll() {
                Ok(Async::Ready(buf)) => {
                    self.range.start += buf.len() as u64;
                    Ok(Async::Ready(Some(buf.into())))
                },
                Ok(Async::NotReady) => {
                    self.fut = Some(fut);
                    Ok(Async::NotReady)
                },
                Err(err) => Err(err.error)
            }
        } else {
            Ok(Async::Ready(None))
        }
    }
}
