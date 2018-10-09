use std::io::{ self, Initializer };
use std::os::unix::io::{ AsRawFd, RawFd };
use bytes::{ Buf, BufMut };
use rustls::ServerSession;
use tokio::prelude::*;
use tokio_rustls::{ TlsAcceptor, TlsStream };
use tokio_rusktls::KtlsStream;


pub enum Stream<IO> {
    Socket(IO),
    Tls(TlsStream<IO, ServerSession>),
    Ktls(KtlsStream<IO>)
}

pub enum InnerAccept<IO, Fut> {
    Socket(Option<IO>),
    Fut(Fut),
}

impl<IO> Stream<IO>
where IO: AsyncRead + AsyncWrite + AsRawFd
{
    pub fn new(io: IO, accept: Option<TlsAcceptor>)
        -> InnerAccept<IO, impl Future<Item=Stream<IO>, Error=io::Error>>
    {
        if let Some(acceptor) = accept {
            let fut = acceptor.accept(io)
                .and_then(|stream| {
                    let (io, session) = stream.into_inner();
                    KtlsStream::new(io, &session)
                        .map(Stream::Ktls)
                        .or_else(|err| {
                            eprintln!("warn: {:?}", err.error);

                            let stream = TlsStream::from((err.inner, session));
                            Ok(Stream::Tls(stream))
                        })
                });

            InnerAccept::Fut(fut)
        } else {
            InnerAccept::Socket(Some(io))
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
where IO: AsyncRead + AsyncWrite + AsRawFd
{
    unsafe fn initializer(&self) -> Initializer {
        match self {
            Stream::Socket(io) => io.initializer(),
            Stream::Tls(io) => io.initializer(),
            Stream::Ktls(io) => io.initializer()
        }
    }

    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        match self {
            Stream::Socket(io) => io.read(buf),
            Stream::Tls(io) => io.read(buf),
            Stream::Ktls(io) => io.read(buf)
        }
    }
}

impl<IO> io::Write for Stream<IO>
where IO: AsyncRead + AsyncWrite + AsRawFd
{
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        match self {
            Stream::Socket(io) => io.write(buf),
            Stream::Tls(io) => io.write(buf),
            Stream::Ktls(io) => io.write(buf)
        }
    }

    fn flush(&mut self) -> io::Result<()> {
        match self {
            Stream::Socket(io) => io.flush(),
            Stream::Tls(io) => io.flush(),
            Stream::Ktls(io) => io.flush()
        }
    }
}

impl<IO> AsyncRead for Stream<IO>
where IO: AsyncRead + AsyncWrite + AsRawFd
{
    unsafe fn prepare_uninitialized_buffer(&self, buf: &mut [u8]) -> bool {
        match self {
            Stream::Socket(io) => io.prepare_uninitialized_buffer(buf),
            Stream::Tls(io) => io.prepare_uninitialized_buffer(buf),
            Stream::Ktls(io) => io.prepare_uninitialized_buffer(buf)
        }
    }

    fn read_buf<B: BufMut>(&mut self, buf: &mut B) -> Poll<usize, io::Error> {
        match self {
            Stream::Socket(io) => io.read_buf(buf),
            Stream::Tls(io) => io.read_buf(buf),
            Stream::Ktls(io) => io.read_buf(buf)
        }
    }
}

impl<IO> AsyncWrite for Stream<IO>
where IO: AsyncRead + AsyncWrite + AsRawFd
{
    fn write_buf<B: Buf>(&mut self, buf: &mut B) -> Poll<usize, io::Error> {
        match self {
            Stream::Socket(io) => io.write_buf(buf),
            Stream::Tls(io) => io.write_buf(buf),
            Stream::Ktls(io) => io.write_buf(buf)
        }
    }

    fn shutdown(&mut self) -> Poll<(), io::Error> {
        match self {
            Stream::Socket(io) => io.shutdown(),
            Stream::Tls(io) => io.shutdown(),
            Stream::Ktls(io) => io.shutdown()
        }
    }
}

impl<IO: AsRawFd> AsRawFd for Stream<IO> {
    fn as_raw_fd(&self) -> RawFd {
        match self {
            Stream::Socket(io) => io.as_raw_fd(),
            Stream::Tls(io) => {
                let (io, _) = io.get_ref();
                io.as_raw_fd()
            },
            Stream::Ktls(io) => io.as_raw_fd()
        }
    }
}
