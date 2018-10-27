use std::io::{ self, Initializer };
use bytes::{ Buf, BufMut };
use log::{ log, warn };
use rustls::{ Session, ServerSession };
use tokio::prelude::*;
use tokio_rustls::{ TlsAcceptor, TlsStream };

#[cfg(unix)]
use std::os::unix::io::{ AsRawFd, RawFd };

#[cfg(target_os = "linux")]
use tokio_rusktls::KtlsStream;


pub enum Stream<IO> {
    Socket(IO),
    Tls(TlsStream<IO, ServerSession>),
    #[cfg(target_os = "linux")]
    Ktls(Option<String>, KtlsStream<IO>)
}

pub enum InnerAccept<IO, Fut> {
    Socket(Option<IO>),
    Fut(Fut),
}

impl<IO> Stream<IO>
where IO: private::AsyncIO
{
    pub fn new(io: IO, accept: Option<TlsAcceptor>)
        -> InnerAccept<IO, impl Future<Item=Self, Error=io::Error>>
    {
        if let Some(acceptor) = accept {
            #[cfg(not(target_os = "linux"))]
            let fut = acceptor.accept(io).map(Stream::Tls);

            #[cfg(target_os = "linux")]
            let fut = acceptor.accept(io)
                .and_then(|stream| {
                    let (io, session) = stream.into_inner();
                    KtlsStream::new(io, &session)
                        .map(|kstream| {
                            let protocol = session.get_alpn_protocol().map(ToOwned::to_owned);
                            Stream::Ktls(protocol, kstream)
                        })
                        .or_else(|err| {
                            warn!("socket/ktls: {:?}", err.error);

                            let stream = TlsStream::from((err.inner, session));
                            Ok(Stream::Tls(stream))
                        })
                });

            InnerAccept::Fut(fut)
        } else {
            InnerAccept::Socket(Some(io))
        }
    }

    pub fn get_alpn_protocol(&self) -> Option<&str> {
        match self {
            Stream::Socket(_) => None,
            Stream::Tls(io) => {
                let (_, session) = io.get_ref();
                session.get_alpn_protocol()
            },
            #[cfg(target_os = "linux")]
            Stream::Ktls(protocol, _) => protocol
                .as_ref()
                .map(String::as_str)
        }
    }

    pub fn is_sendable(&self) -> bool {
        match self {
            Stream::Socket(_) => true,
            #[cfg(target_os = "linux")]
            Stream::Ktls(..) => true,
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
            Stream::Tls(io) => io.initializer(),
            #[cfg(target_os = "linux")]
            Stream::Ktls(_, io) => io.initializer()
        }
    }

    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        match self {
            Stream::Socket(io) => io.read(buf),
            Stream::Tls(io) => io.read(buf),
            #[cfg(target_os = "linux")]
            Stream::Ktls(_, io) => io.read(buf)
        }
    }
}

impl<IO> io::Write for Stream<IO>
where IO: private::AsyncIO
{
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        match self {
            Stream::Socket(io) => io.write(buf),
            Stream::Tls(io) => io.write(buf),
            #[cfg(target_os = "linux")]
            Stream::Ktls(_, io) => io.write(buf)
        }
    }

    fn flush(&mut self) -> io::Result<()> {
        match self {
            Stream::Socket(io) => io.flush(),
            Stream::Tls(io) => io.flush(),
            #[cfg(target_os = "linux")]
            Stream::Ktls(_, io) => io.flush()
        }
    }
}

impl<IO> AsyncRead for Stream<IO>
where IO: private::AsyncIO
{
    unsafe fn prepare_uninitialized_buffer(&self, buf: &mut [u8]) -> bool {
        match self {
            Stream::Socket(io) => io.prepare_uninitialized_buffer(buf),
            Stream::Tls(io) => io.prepare_uninitialized_buffer(buf),
            #[cfg(target_os = "linux")]
            Stream::Ktls(_, io) => io.prepare_uninitialized_buffer(buf)
        }
    }

    fn read_buf<B: BufMut>(&mut self, buf: &mut B) -> Poll<usize, io::Error> {
        match self {
            Stream::Socket(io) => io.read_buf(buf),
            Stream::Tls(io) => io.read_buf(buf),
            #[cfg(target_os = "linux")]
            Stream::Ktls(_, io) => io.read_buf(buf)
        }
    }
}

impl<IO> AsyncWrite for Stream<IO>
where IO: private::AsyncIO
{
    fn write_buf<B: Buf>(&mut self, buf: &mut B) -> Poll<usize, io::Error> {
        match self {
            Stream::Socket(io) => io.write_buf(buf),
            Stream::Tls(io) => io.write_buf(buf),
            #[cfg(target_os = "linux")]
            Stream::Ktls(_, io) => io.write_buf(buf)
        }
    }

    fn shutdown(&mut self) -> Poll<(), io::Error> {
        match self {
            Stream::Socket(io) => io.shutdown(),
            Stream::Tls(io) => io.shutdown(),
            #[cfg(target_os = "linux")]
            Stream::Ktls(_, io) => io.shutdown()
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
            },
            #[cfg(target_os = "linux")]
            Stream::Ktls(_, io) => io.as_raw_fd()
        }
    }
}

#[cfg(unix)]
mod private {
    use super::*;

    pub trait AsyncIO: AsyncRead + AsyncWrite + AsRawFd {}
    impl<T: AsyncRead + AsyncWrite + AsRawFd> AsyncIO for T {}
}

#[cfg(not(unix))]
mod private {
    use super::*;

    pub trait AsyncIO: AsyncRead + AsyncWrite {}
    impl<T: AsyncRead + AsyncWrite> AsyncIO for T {}
}
