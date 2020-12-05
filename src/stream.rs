use std::pin::Pin;
use std::marker::Unpin;
use std::io::{ self, IoSlice };
use std::task::{ Context, Poll };
use tokio::io::{ AsyncRead, AsyncWrite, ReadBuf };
use tokio_rustls::{ TlsAcceptor, server::TlsStream };


pub enum Stream<IO> {
    Socket(IO),
    Tls(TlsStream<IO>)
}

impl<IO> Stream<IO>
where IO: private::AsyncIO
{
    pub async fn new(io: IO, accept: Option<TlsAcceptor>) -> io::Result<Stream<IO>> {
        Ok(match accept {
            Some(acceptor) => Stream::Tls(acceptor.accept(io).await?),
            None => Stream::Socket(io)
        })
    }
}

impl<IO: private::AsyncIO> AsyncRead for Stream<IO> {
    #[inline]
    fn poll_read(self: Pin<&mut Self>, cx: &mut Context<'_>, buf: &mut ReadBuf<'_>) -> Poll<io::Result<()>> {
        match self.get_mut() {
            Stream::Socket(io) => Pin::new(io).poll_read(cx, buf),
            Stream::Tls(io) => Pin::new(io).poll_read(cx, buf)
        }
    }
}

impl<IO: private::AsyncIO> AsyncWrite for Stream<IO> {
    #[inline]
    fn poll_write(self: Pin<&mut Self>, cx: &mut Context<'_>, buf: &[u8]) -> Poll<io::Result<usize>> {
        match self.get_mut() {
            Stream::Socket(io) => Pin::new(io).poll_write(cx, buf),
            Stream::Tls(io) => Pin::new(io).poll_write(cx, buf)
        }
    }

    #[inline]
    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        match self.get_mut() {
            Stream::Socket(io) => Pin::new(io).poll_flush(cx),
            Stream::Tls(io) => Pin::new(io).poll_flush(cx)
        }
    }

    #[inline]
    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        match self.get_mut() {
            Stream::Socket(io) => Pin::new(io).poll_shutdown(cx),
            Stream::Tls(io) => Pin::new(io).poll_shutdown(cx)
        }
    }

    #[inline]
    fn poll_write_vectored(self: Pin<&mut Self>, cx: &mut Context<'_>, bufs: &[IoSlice<'_>])
        -> Poll<io::Result<usize>>
    {
        match self.get_mut() {
            Stream::Socket(io) => Pin::new(io).poll_write_vectored(cx, bufs),
            Stream::Tls(io) => Pin::new(io).poll_write_vectored(cx, bufs)
        }
    }

    #[inline]
    fn is_write_vectored(&self) -> bool {
        match &*self {
            Stream::Socket(io) => io.is_write_vectored(),
            Stream::Tls(io) => io.is_write_vectored()
        }
    }
}

mod private {
    use super::*;

    pub trait AsyncIO: AsyncRead + AsyncWrite + Unpin {}
    impl<T: AsyncRead + AsyncWrite + Unpin> AsyncIO for T {}
}
