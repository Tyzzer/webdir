use std::fs;
use std::pin::Pin;
use std::path::Path;
use std::task::{ Context, Poll };
use std::io::{ self, Read, Seek };
use tokio::task::block_in_place;
use tokio::io::{ AsyncRead, ReadBuf };


pub struct File(fs::File);

impl File {
    pub async fn open(path: &Path) -> io::Result<File> {
        block_in_place(|| fs::File::open(path).map(File))
    }

    pub async fn seek(&mut self, pos: io::SeekFrom) -> io::Result<u64> {
        block_in_place(|| self.0.seek(pos))
    }
}

impl AsyncRead for File {
    #[inline]
    fn poll_read(self: Pin<&mut Self>, _cx: &mut Context<'_>, buf: &mut ReadBuf<'_>) -> Poll<io::Result<()>> {
        let n = block_in_place(|| self.get_mut().0.read(buf.initialize_unfilled()))?;
        buf.advance(n);
        Poll::Ready(Ok(()))
    }
}
