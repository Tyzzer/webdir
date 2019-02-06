use std::io::{ self, Initializer };
use bytes::{ Buf, BufMut };
use rustls::{ Session, ServerSession };
use tokio::prelude::*;
use tokio_rustls::{ TlsAcceptor, TlsStream };

#[cfg(unix)]
use std::os::unix::io::{ AsRawFd, RawFd };


pub enum Stream<IO> {
    Socket(IO),
    Tls(TlsStream<IO, ServerSession>)
}

pub enum InnerAccept<IO, Fut> {
    Socket(Option<IO>),
    Fut(Fut)
}

impl<IO> Stream<IO>
where IO: private::AsyncIO
{
    #[allow(clippy::new_ret_no_self)]
    pub fn new(io: IO, accept: Option<TlsAcceptor>)
        -> InnerAccept<IO, impl Future<Item=Self, Error=io::Error>>
    {
        if let Some(acceptor) = accept {
            InnerAccept::Fut(acceptor.accept(io).map(Stream::Tls))
        } else {
            InnerAccept::Socket(Some(io))
        }
    }

    pub fn get_alpn_protocol(&self) -> Option<&[u8]> {
        match self {
            Stream::Socket(_) => None,
            Stream::Tls(io) => {
                let (_, session) = io.get_ref();
                session.get_alpn_protocol()
            }
        }
    }

    pub fn is_sendable(&self) -> bool {
        match self {
            Stream::Socket(_) => true,
            _ => false
        }
    }
}

impl<IO, Fut> Future for InnerAccept<IO, Fut>
where Fut: Future<Item=Stream<IO>, Error=io::Error>
{
    type Item = Fut::Item;
    type Error = Fut::Error;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        match self {
            InnerAccept::Socket(io) => {
                let io = io.take().unwrap();
                Ok(Async::Ready(Stream::Socket(io)))
            },
            InnerAccept::Fut(fut) => fut.poll()
        }
    }
}

impl<IO> io::Read for Stream<IO>
where IO: private::AsyncIO
{
    unsafe fn initializer(&self) -> Initializer {
        match self {
            Stream::Socket(io) => io.initializer(),
            Stream::Tls(io) => io.initializer()
        }
    }

    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        match self {
            Stream::Socket(io) => io.read(buf),
            Stream::Tls(io) => io.read(buf)
        }
    }
}

impl<IO> io::Write for Stream<IO>
where IO: private::AsyncIO
{
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        match self {
            Stream::Socket(io) => io.write(buf),
            Stream::Tls(io) => io.write(buf)
        }
    }

    fn flush(&mut self) -> io::Result<()> {
        match self {
            Stream::Socket(io) => io.flush(),
            Stream::Tls(io) => io.flush()
        }
    }
}

impl<IO> AsyncRead for Stream<IO>
where IO: private::AsyncIO
{
    unsafe fn prepare_uninitialized_buffer(&self, buf: &mut [u8]) -> bool {
        match self {
            Stream::Socket(io) => io.prepare_uninitialized_buffer(buf),
            Stream::Tls(io) => io.prepare_uninitialized_buffer(buf)
        }
    }

    fn read_buf<B: BufMut>(&mut self, buf: &mut B) -> Poll<usize, io::Error> {
        match self {
            Stream::Socket(io) => io.read_buf(buf),
            Stream::Tls(io) => io.read_buf(buf)
        }
    }
}

impl<IO> AsyncWrite for Stream<IO>
where IO: private::AsyncIO
{
    fn write_buf<B: Buf>(&mut self, buf: &mut B) -> Poll<usize, io::Error> {
        match self {
            Stream::Socket(io) => io.write_buf(buf),
            Stream::Tls(io) => io.write_buf(buf)
        }
    }

    fn shutdown(&mut self) -> Poll<(), io::Error> {
        match self {
            Stream::Socket(io) => io.shutdown(),
            Stream::Tls(io) => io.shutdown()
        }
    }
}

#[cfg(unix)]
impl<IO: AsRawFd> AsRawFd for Stream<IO> {
    fn as_raw_fd(&self) -> RawFd {
        match self {
            Stream::Socket(io) => io.as_raw_fd(),
            Stream::Tls(io) => {
                let (io, _) = io.get_ref();
                io.as_raw_fd()
            }
        }
    }
}

mod private {
    use super::*;

    pub trait AsyncIO: AsyncRead + AsyncWrite {}
    impl<T: AsyncRead + AsyncWrite> AsyncIO for T {}
}
