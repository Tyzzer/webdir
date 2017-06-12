use std::{ io, fmt };
use std::sync::Arc;
use std::time::UNIX_EPOCH;
use std::cmp::Ordering;
use std::ffi::OsString;
use std::path::{ PathBuf, Path };
use std::fs::{ DirEntry, ReadDir, Metadata };
use maud::{ Render, Markup };
use chrono::{ TimeZone, UTC };
use humanesort::HumaneOrder;
use ::utils::encode_path;


pub type IoRREntry = io::Result<io::Result<Entry>>;
const SORTDIR_BUFF_LENGTH: usize = 100;

pub struct SortDir {
    root: Arc<PathBuf>,
    readdir: ReadDir,
    buf: Vec<IoRREntry>
}

impl SortDir {
    pub fn new(root: Arc<PathBuf>, mut readdir: ReadDir) -> Self {
        fn sort_by_entry(x: &IoRREntry, y: &IoRREntry) -> Ordering {
            if let (&Ok(Ok(ref x)), &Ok(Ok(ref y))) = (x, y) {
                match Ord::cmp(&x.ty, &y.ty) {
                    Ordering::Equal => HumaneOrder::humane_cmp(&x.name.to_string_lossy(), &y.name.to_string_lossy()),
                    order => order
                }
            } else {
                Ordering::Equal
            }
        }

        let mut buf = readdir
            .by_ref()
            .map(|entry| entry.map(|p| Entry::new(&root, p)))
            .take(SORTDIR_BUFF_LENGTH)
            .collect::<Vec<_>>();
        buf.sort_unstable_by(|x, y| sort_by_entry(y, x));

        SortDir { root, readdir, buf }
    }
}

impl Iterator for SortDir {
    type Item = IoRREntry;

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


#[derive(PartialEq, Eq, PartialOrd, Ord)]
pub enum EntryType {
    Dir,
    File,
    Symlink,
    Other
}

impl fmt::Display for EntryType {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        use std::fmt::Write;

        f.write_char(match *self {
            EntryType::Dir => '📁',
            EntryType::File => '📄',
            EntryType::Symlink => '🔁',
            EntryType::Other => '❓'
        })
    }
}

pub struct Entry {
    pub metadata: Metadata,
    pub name: OsString,
    pub uri: Option<String>,
    pub ty: EntryType
}

impl Entry {
    pub fn new(base: &Path, entry: DirEntry) -> io::Result<Self> {
        let mut metadata = entry.metadata()?;
        let path = entry.path();
        let name = entry.file_name();
        let is_symlink = metadata.file_type().is_symlink();
        if is_symlink {
            metadata = path.metadata()?;
        }

        let ty = if is_symlink {
            EntryType::Symlink
        } else {
            let ty = metadata.file_type();
            if ty.is_dir() {
                EntryType::Dir
            } else if ty.is_file() {
                EntryType::File
            } else if ty.is_symlink() {
                EntryType::Symlink
            } else {
                EntryType::Other
            }
        };

        let uri = path.strip_prefix(base)
            .map(encode_path)
            .map(|p| if metadata.is_dir() { p + "/" } else { p })
            .ok();

        Ok(Entry { metadata, name, uri, ty })
    }

    pub fn time(&self) -> io::Result<String> {
        self.metadata.modified()
            .and_then(|time| time.duration_since(UNIX_EPOCH)
                .map_err(|err| err!(Other, err))
            )
            .map(|dur| UTC.timestamp(dur.as_secs() as _, 0))
            .map(|time| time.to_string())
    }

    pub fn size(&self) -> String {
        use humansize::FileSize;
        use humansize::file_size_opts::BINARY;

        FileSize::file_size(&self.metadata.len(), BINARY)
            .unwrap_or_else(|err| err)
    }
}

impl Render for Entry {
    fn render(&self) -> Markup {
        html!{
            tr {
                td class="icon" (self.ty)

                td class="link" @if let Some(ref uri) = self.uri {
                    a href=(uri) (self.name.to_string_lossy())
                } @else {
                    (self.name.to_string_lossy())
                }

                td small class="time" @if let Ok(time) = self.time() {
                    (time)
                } @else {
                    "-"
                }

                td class="size" @if let EntryType::File = self.ty {
                    (self.size())
                } @else {
                    "-"
                }
            }
        }
    }
}

#[inline]
pub fn up(top: bool) -> Markup {
    html!{
        tr {
            td  class="icon"    "⤴️"
            td  class="link"    @if !top { a href=".." ".." }
        }
    }
}
