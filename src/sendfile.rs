use std::{ io, fs };
use std::sync::Arc;
use std::os::unix::io::AsRawFd;
use libc::off_t;
use bytes::buf::{ Buf, BufMut };
use futures::{ Poll, Stream, Async };
use futures::sync::{ BiLock, BiLockAcquired };
use tokio_io::{ AsyncRead, AsyncWrite };
use tokio_core::net::TcpStream;
use nix;
use ::error;


pub struct BiTcpStream(pub BiLockAcquired<TcpStream>);

impl io::Read for BiTcpStream {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.0.read(buf)
    }
}

impl io::Write for BiTcpStream {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.0.write(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.0.flush()
    }
}

impl AsyncRead for BiTcpStream {
    unsafe fn prepare_uninitialized_buffer(&self, buf: &mut [u8]) -> bool {
        AsyncRead::prepare_uninitialized_buffer(&self.0 as &TcpStream, buf)
    }

    fn read_buf<B: BufMut>(&mut self, buf: &mut B) -> Poll<usize, io::Error> {
        AsyncRead::read_buf(&mut self.0 as &mut TcpStream, buf)
    }
}

impl AsyncWrite for BiTcpStream {
    fn shutdown(&mut self) -> Poll<(), io::Error> {
        AsyncWrite::shutdown(&mut self.0 as &mut TcpStream)
    }

    fn write_buf<B: Buf>(&mut self, buf: &mut B) -> Poll<usize, io::Error> {
        AsyncWrite::write_buf(&mut self.0 as &mut TcpStream, buf)
    }
}


pub struct SendFileFut {
    pub socket: Arc<BiLock<TcpStream>>,
    pub fd: fs::File,
    pub offset: off_t,
    pub count: usize
}

impl Stream for SendFileFut {
    type Item = usize;
    type Error = error::Error;

    fn poll(&mut self) -> Poll<Option<Self::Item>, Self::Error> {
        #[cfg(all(unix, not(any(target_os = "macos", target_os = "ios"))))]
        use nix::sys::sendfile::sendfile;

        #[cfg(any(target_os = "macos", target_os = "ios"))]
        use self::macos::sendfile;


        if self.count == 0 {
            return Ok(Async::Ready(None))
        }

        let socket = match self.socket.poll_lock() {
            Async::Ready(socket) => socket,
            Async::NotReady => return Ok(Async::NotReady)
        };

        if let Async::NotReady = socket.poll_write() {
            return Ok(Async::NotReady)
        }

        match sendfile(
            socket.as_raw_fd(), self.fd.as_raw_fd(),
            Some(&mut self.offset), self.count
        ) {
            Ok(read_len) => {
                self.count -= read_len;
                Ok(Async::Ready(Some(read_len)))
            },
            Err(ref err) if nix::Errno::EAGAIN == err.errno() => {
                // TODO https://github.com/tokio-rs/tokio-core/issues/196
                // socket.need_write();

                Ok(Async::NotReady)
            },
            Err(err) => Err(err.into())
        }
    }
}


#[cfg(any(target_os = "macos", target_os = "ios"))]
mod macos {
    use std::ptr;
    use std::os::unix::io::RawFd;
    use libc::{ off_t, sendfile as raw_sendfile };
    use nix;

    pub fn sendfile(out_fd: RawFd, in_fd: RawFd, offset: Option<&mut off_t>, count: usize) -> nix::Result<usize> {
        let &mut offset = offset.unwrap_or(&mut 0);
        let mut len = count as _;
        match unsafe { raw_sendfile(out_fd, in_fd, offset, &mut len, ptr::null_mut(), 0) } {
            0 => Ok(count - len as usize),
            _ => Err(nix::Error::last())
        }
    }
}
