use std::io;
use std::cmp::Ordering;
use std::path::PathBuf;
use std::fs::{ ReadDir, Metadata };
use std::sync::Arc;
use humanesort::HumaneOrder;
use ::render::Entry;


pub type RREntry = io::Result<io::Result<Entry>>;

pub struct SortDir {
    root: Arc<PathBuf>,
    readdir: ReadDir,
    buf: Vec<RREntry>
}

impl SortDir {
    pub fn new(root: Arc<PathBuf>, mut readdir: ReadDir) -> Self {
        let mut buf = readdir
            .by_ref()
            .map(|entry| entry.map(|p| Entry::new(&root, p)))
            .take(100)
            .collect::<Vec<_>>();
        buf.sort_by(|x, y| sort_by_entry(y, x));

        SortDir { root, readdir, buf }
    }
}

impl Iterator for SortDir {
    type Item = RREntry;

    fn next(&mut self) -> Option<Self::Item> {
        if let Some(entry) = self.buf.pop() {
            Some(entry)
        } else {
            self.readdir
                .next()
                .map(|entry| entry
                    .map(|p| Entry::new(&self.root, p))
                )
        }
    }
}

#[inline]
fn file_type(metadata: &Metadata) -> u8 {
    let ty = metadata.file_type();
    if ty.is_dir() {
        0
    } else if ty.is_file() {
        1
    } else if ty.is_symlink() {
        2
    } else {
        3
    }
}

fn sort_by_entry(x: &RREntry, y: &RREntry) -> Ordering {
    if let (&Ok(Ok(ref x)), &Ok(Ok(ref y))) = (x, y) {
        match file_type(&x.metadata).cmp(&file_type(&y.metadata)) {
            Ordering::Equal => HumaneOrder::humane_cmp(&x.name, &y.name),
            order => order
        }
    } else {
        Ordering::Equal
    }
}
