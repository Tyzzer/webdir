use std::io;
use std::cmp::Ordering;
use std::path::PathBuf;
use std::fs::ReadDir;
use std::sync::Arc;
use humanesort::HumaneOrder;
use ::render::Entry;


pub type RREntry = io::Result<io::Result<Entry>>;
const SORTDIR_BUFF_LENGTH: usize = 100;

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
            .take(SORTDIR_BUFF_LENGTH)
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

fn sort_by_entry(x: &RREntry, y: &RREntry) -> Ordering {
    if let (&Ok(Ok(ref x)), &Ok(Ok(ref y))) = (x, y) {
        match Ord::cmp(&x.ty(), &y.ty()) {
            Ordering::Equal => HumaneOrder::humane_cmp(&x.name, &y.name),
            order => order
        }
    } else {
        Ordering::Equal
    }
}
