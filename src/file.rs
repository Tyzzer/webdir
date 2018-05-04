use std::{ io, cmp };
use std::ops::Range;
use std::io::SeekFrom;
use futures::{ Stream, Poll, Async };
use tokio_io::AsyncRead;
use tokio_io::io::Window;
use tokio::fs::file::File as TokioFile;
use hyper;
use ::error;

#[cfg(feature = "sendfile")] use tokio::net::TcpStream;
#[cfg(feature = "sendfile")] use std::sync::Arc;
#[cfg(feature = "sendfile")] use futures::sync::BiLock;
#[cfg(feature = "sendfile")] use ::sendfile::SendFileFut;


pub struct File {
    fd: TokioFile,
    chunk_length: usize,
    pub length: u64
}

impl File {
    #[inline]
    pub fn new(fd: TokioFile, chunk_length: usize, length: u64) -> Self {
        File { fd, chunk_length, length }
    }

    pub fn read(self, range: Range<u64>) -> ReadStream {
        let take = range.end - range.start;
        let buf = vec![0; cmp::min(self.chunk_length, take as _)];
        ReadStream { fd: self.fd, range, buf }
    }

    #[cfg(feature = "sendfile")]
    #[cfg_attr(feature = "cargo-clippy", allow(needless_pass_by_value))]
    pub fn sendfile(self, range: Range<u64>, socket: Arc<BiLock<TcpStream>>) -> SendFileFut {
        use std::{ fs, mem };

        unsafe fn as_fs_file(t: TokioFile) -> fs::File {
            struct PubTokioFile {
                std: Option<fs::File>
            }

            mem::transmute::<_, PubTokioFile>(t).std.unwrap()
        }

        SendFileFut {
            socket,
            fd: unsafe { as_fs_file(self.fd) },
            offset: range.start as _,
            end: range.end as _
        }
    }
}

#[inline]
pub fn try_clone(fd: TokioFile) -> TryClone {
    TryClone(fd)
}

pub struct TryClone(TokioFile);

impl Stream for TryClone {
    type Item = TokioFile;
    type Error = io::Error;

    fn poll(&mut self) -> Poll<Option<Self::Item>, Self::Error> {
        let fd = try_ready!(self.0.poll_try_clone());
        Ok(Async::Ready(Some(fd)))
    }
}

pub struct ReadStream {
    fd: TokioFile,
    range: Range<u64>,
    buf: Vec<u8>,
}

impl Stream for ReadStream {
    type Item = hyper::Result<hyper::Chunk>;
    type Error = error::Error;

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
            Ok(Async::Ready(Some(Ok(chunk.into()))))
        } else {
            Ok(Async::Ready(None))
        }
    }
}


#[cfg(test)]
mod test {
    extern crate tempdir;

    use std::fs;
    use std::io::Write;
    use futures::{ Future, Stream };
    use tokio::fs::File as TokioFile;
    use self::tempdir::TempDir;
    use super::*;

    #[test]
    fn test_file() {
        let tmp = TempDir::new("webdir_test_file").unwrap();

        {
            fs::File::create(tmp.path().join("test")).unwrap()
                .write_all(&[42; 1024]).unwrap();
        }

        let done = TokioFile::open(tmp.path().join("test"))
            .map_err(Into::into)
            .and_then(|fd| File::new(fd, 1 << 8, 989).read(32..1021)
                .map(|chunk| chunk.unwrap().to_vec())
                .concat2()
            )
            .map(|output| assert_eq!(output, &[42; 989][..]))
            .map_err(|err| panic!("{:?}", err));

        ::tokio::run(done);
    }
}
