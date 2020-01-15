use std::io;
use std::pin::Pin;
use std::marker::Unpin;
use std::future::Future;
use std::mem::MaybeUninit;
use std::task::{ Context, Poll };
use bytes::{ Buf, BufMut };
use tokio::io::{ AsyncRead, AsyncWrite };
use tokio_rustls::{ TlsAcceptor, Accept, server::TlsStream };


pub enum Stream<IO> {
    Socket(IO),
    Tls(TlsStream<IO>)
}

pub enum AllAccept<IO> {
    Socket(Option<IO>),
    Fut(Accept<IO>)
}

impl<IO> Stream<IO>
where IO: private::AsyncIO
{
    #[allow(clippy::new_ret_no_self)]
    pub fn new(io: IO, accept: Option<TlsAcceptor>)
        -> AllAccept<IO>
    {
        if let Some(acceptor) = accept {
            AllAccept::Fut(acceptor.accept(io))
        } else {
            AllAccept::Socket(Some(io))
        }
    }
}

impl<IO: private::AsyncIO> Future for AllAccept<IO> {
    type Output = io::Result<Stream<IO>>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        match self.get_mut() {
            AllAccept::Socket(io) => {
                let io = io.take().unwrap();
                Poll::Ready(Ok(Stream::Socket(io)))
            },
            AllAccept::Fut(fut) => Pin::new(fut).poll(cx).map_ok(Stream::Tls)
        }
    }
}

impl<IO: private::AsyncIO> AsyncRead for Stream<IO> {
    #[inline]
    unsafe fn prepare_uninitialized_buffer(&self, buf: &mut [MaybeUninit<u8>]) -> bool {
        match self {
            Stream::Socket(io) => io.prepare_uninitialized_buffer(buf),
            Stream::Tls(io) => io.prepare_uninitialized_buffer(buf)
        }
    }

    #[inline]
    fn poll_read(self: Pin<&mut Self>, cx: &mut Context<'_>, buf: &mut [u8]) -> Poll<io::Result<usize>> {
        match self.get_mut() {
            Stream::Socket(io) => Pin::new(io).poll_read(cx, buf),
            Stream::Tls(io) => Pin::new(io).poll_read(cx, buf)
        }
    }

    #[inline]
    fn poll_read_buf<B: BufMut>(self: Pin<&mut Self>, cx: &mut Context<'_>, buf: &mut B) -> Poll<io::Result<usize>> {
        match self.get_mut() {
            Stream::Socket(io) => Pin::new(io).poll_read_buf(cx, buf),
            Stream::Tls(io) => Pin::new(io).poll_read_buf(cx, buf)
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
    fn poll_write_buf<B: Buf>(self: Pin<&mut Self>, cx: &mut Context<'_>, buf: &mut B) -> Poll<io::Result<usize>> {
        match self.get_mut() {
            Stream::Socket(io) => Pin::new(io).poll_write_buf(cx, buf),
            Stream::Tls(io) => Pin::new(io).poll_write_buf(cx, buf)
        }
    }


}

mod private {
    use super::*;

    pub trait AsyncIO: AsyncRead + AsyncWrite + Unpin {}
    impl<T: AsyncRead + AsyncWrite + Unpin> AsyncIO for T {}
}
