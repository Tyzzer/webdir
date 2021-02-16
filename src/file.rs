use std::fs;
use std::path::Path;
use bytes::Bytes;
use std::io::{ self, Read, Seek };
use tokio::task::block_in_place;


pub struct File {
    inner: fs::File,
    buf: Vec<u8>,
}

impl File {
    pub async fn open(path: &Path) -> io::Result<File> {
        block_in_place(|| {
            let fd = fs::File::open(path)?;
            Ok(File {
                inner: fd,
                buf: vec![0; 1 << 16],
            })
        })
    }

    pub async fn seek(&mut self, pos: io::SeekFrom) -> io::Result<u64> {
        block_in_place(|| self.inner.seek(pos))
    }

    pub async fn next_chunk(&mut self) -> io::Result<Option<Bytes>> {
        block_in_place(|| {
            let n = self.inner.read(&mut self.buf)?;
            Ok(if n == 0 {
                None
            } else {
                Some(Bytes::from(&self.buf[..n]))
            })
        })
    }
}
