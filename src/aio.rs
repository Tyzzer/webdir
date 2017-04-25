use std::{ io, mem, ptr, cmp };
use std::fs::File;
use std::ops::Range;
use std::os::unix::io::AsRawFd;
use libc::{
    ECANCELED, EINPROGRESS, LIO_READ, SIGEV_NONE,
    off_t, c_void as void,
    aiocb as AioCb, sigevent as SigEvent,
    aio_read, aio_error, aio_return
};
use bytes::BytesMut;
use futures::{ Stream, Async, Poll };


pub const CHUNK_SIZE: usize = 1024;

pub struct AioReadBuf {
    fd: File,
    buf: BytesMut,
    aiocb: Option<AioCb>,
    range: Range<usize>
}


impl AioReadBuf {
    pub fn new(fd: File, range: Range<usize>) -> Self {
        assert!(range.start <= range.end);
        AioReadBuf {
            fd, range,
            buf: BytesMut::from(vec![0; CHUNK_SIZE]),
            aiocb: None,
        }
    }
}

impl Stream for AioReadBuf {
    type Item = BytesMut;
    type Error = ::hyper::Error;

    fn poll(&mut self) -> Poll<Option<Self::Item>, Self::Error> {
        let (is_next, result) = if let Some(ref mut aiocb) = self.aiocb {
            match unsafe { aio_error(aiocb) } {
                0 => match unsafe { aio_return(aiocb) } {
                    -1 => return Err(io::Error::last_os_error().into()),
                    length => {
                        let length = cmp::min(length as usize, self.range.end);
                        self.range.start = cmp::min(self.range.start + length, self.range.end);
                        (true, Ok(Async::Ready(Some(self.buf[..length].into()))))
                    }
                },
                ECANCELED => return Err(err!(Interrupted).into()),
                EINPROGRESS => return Ok(Async::NotReady),
                err => return Err(err!(os err).into())
            }
        } else {
            if self.range.start < self.range.end {
                let mut sigevent: SigEvent = unsafe { mem::zeroed() };
                sigevent.sigev_notify = SIGEV_NONE;
                sigevent.sigev_value.sival_ptr = ptr::null_mut();

                let mut aiocb: AioCb = unsafe { mem::zeroed() };
                aiocb.aio_fildes = self.fd.as_raw_fd();
                aiocb.aio_offset = self.range.start as off_t;
                aiocb.aio_nbytes = cmp::min(self.range.end - self.range.start, CHUNK_SIZE);
                aiocb.aio_buf = self.buf.as_ptr() as *mut void;
                aiocb.aio_reqprio = 0;
                aiocb.aio_lio_opcode = LIO_READ;
                aiocb.aio_sigevent = sigevent;

                if unsafe { aio_read(&mut aiocb) } == 0 {
                    self.aiocb = Some(aiocb);
                    return Ok(Async::NotReady);
                } else {
                    return Err(io::Error::last_os_error().into());
                }
            } else {
                return Ok(Async::Ready(None));
            }
        };

        if is_next {
            self.aiocb = None;
        }

        result
    }
}
