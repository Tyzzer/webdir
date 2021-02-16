use std::{ fs, io };
use std::path::Path;
use bytes::{ BytesMut, Bytes };
use once_cell::sync::OnceCell;
use ritsu::actions;


pub struct File {
    inner: Option<fs::File>,
    buf: Option<BytesMut>,
    pos: u64,
    len: u64
}

impl File {
    pub async fn open(path: &Path) -> io::Result<File> {
        let handle = GLOBAL_HANDLE.get_or_init(init_ritsu_runtime);

        let fd = actions::fs::open(handle, path).await?;
        let len = fd.metadata()?.len();

        Ok(File {
            inner: Some(fd),
            buf: Some(BytesMut::with_capacity(1 << 16)),
            pos: 0,
            len
        })
    }

    pub async fn seek(&mut self, pos: io::SeekFrom) -> io::Result<u64> {
        fn add(x: u64, y: i64) -> u64 {
            if y >= 0 {
                x + (y as u64)
            } else {
                x - (-y as u64)
            }
        }

        match pos {
            io::SeekFrom::Start(offset) => self.pos = offset,
            io::SeekFrom::End(offset) => self.pos = add(self.len, offset),
            io::SeekFrom::Current(offset) => self.pos = add(self.pos, offset)
        }

        Ok(self.pos)
    }

    pub async fn next_chunk(&mut self) -> io::Result<Option<Bytes>> {
        let handle = GLOBAL_HANDLE.get_or_init(init_ritsu_runtime);

        let (fd, mut buf) = actions::io::read_buf(
            handle,
            &mut self.inner,
            self.buf.take().unwrap_or_else(|| BytesMut::with_capacity(1 << 16)),
            Some(self.pos as _)
        ).await?;

        self.inner = Some(fd);

        Ok(if buf.is_empty() {
            None
        } else {
            self.pos += buf.len() as u64;
            let buf2 = buf.split();

            if buf.capacity() > 64 {
                self.buf = Some(buf);
            }

            Some(buf2.freeze())
        })
    }
}


use std::thread;
use tokio::sync::mpsc::{ UnboundedSender, unbounded_channel };
use io_uring::squeue;
use ritsu::{ Proactor, Handle };

static GLOBAL_HANDLE: OnceCell<RemoteHandle> = OnceCell::new();

struct RemoteHandle(UnboundedSender<squeue::Entry>);

impl Handle for RemoteHandle {
    unsafe fn push(&self, entry: squeue::Entry) {
        self.0.send(entry).ok().expect("ritsu runtime not found");
    }
}

fn init_ritsu_runtime() -> RemoteHandle {
    let (tx, mut rx) = unbounded_channel();

    thread::spawn(move || {
        let mut proactor = Proactor::new().unwrap();
        let handle = proactor.handle();

        proactor.block_on(async move {
            while let Some(entry) = rx.recv().await {
                unsafe {
                    handle.push(entry);
                }
            }
        }).unwrap();
    });

    RemoteHandle(tx)
}
