use std::{ io, fmt };
use std::cmp::Ordering;
use std::ffi::OsString;
use std::fs::{ DirEntry, ReadDir, Metadata };
use smallvec::SmallVec;
use maud::{ html, Render, Markup };
use chrono::{ Utc, DateTime };
use humanesort::HumaneOrder;
use crate::common::encode_path;


pub const SORTDIR_BUFF_LENGTH: usize = 1 << 12;

pub struct SortDir {
    readdir: ReadDir,
    buf: SmallVec<[io::Result<Entry>; 12]>
}

impl SortDir {
    pub fn new(mut readdir: ReadDir) -> Self {
        fn sort_by_entry(x: &io::Result<Entry>, y: &io::Result<Entry>) -> Ordering {
            if let (&Ok(ref x), &Ok(ref y)) = (x, y) {
                match Ord::cmp(&x.ty, &y.ty) {
                    Ordering::Equal => HumaneOrder::humane_cmp(
                        &x.name.to_string_lossy(),
                        &y.name.to_string_lossy()
                    ),
                    order => order
                }
            } else {
                Ordering::Equal
            }
        }

        let mut buf = readdir
            .by_ref()
            .map(|entry| entry.and_then(Entry::new))
            .take(SORTDIR_BUFF_LENGTH)
            .collect::<SmallVec<_>>();
        buf.sort_unstable_by(|x, y| sort_by_entry(y, x));

        SortDir { readdir, buf }
    }
}

impl Iterator for SortDir {
    type Item = io::Result<Entry>;

    fn next(&mut self) -> Option<Self::Item> {
        self.buf.pop()
            .or_else(|| self.readdir
                .next()
                .map(|entry| entry.and_then(Entry::new))
            )
    }
}


#[derive(PartialEq, Eq, PartialOrd, Ord)]
pub enum EntryType {
    Symlink,
    Dir,
    File,
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
    pub ty: EntryType
}

impl Entry {
    #[allow(clippy::needless_pass_by_value)]
    pub fn new(entry: DirEntry) -> io::Result<Self> {
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

        Ok(Entry { metadata, name, ty })
    }

    #[inline]
    pub fn path(&self) -> String {
        let mut p = encode_path(&self.name);
        if self.metadata.is_dir() {
            p.push('/');
        }
        p
    }

    #[inline]
    pub fn time(&self) -> io::Result<DateTime<Utc>> {
        self.metadata.modified().map(Into::into)
    }

    #[inline]
    pub fn size(&self) -> String {
        use unbytify::bytify;

        let (value, unit) = bytify(self.metadata.len());
        format!("{} {}", value, unit)
    }
}

impl Render for Entry {
    fn render(&self) -> Markup {
        html!{
            tr {
                td class="icon" { (self.ty) }

                td class="link" {
                    a href=(self.path()) { (self.name.to_string_lossy()) }
                }

                td class="time" {
                    small {
                        @if let Ok(time) = self.time() {
                            (time.format("%F %T UTC"))
                        } @else {
                            "-"
                        }
                    }
                }

                td class="size" {
                    @if let EntryType::File = self.ty {
                        (self.size())
                    } @else {
                        "-"
                    }
                }
            }
        }
    }
}

#[inline]
pub fn up(top: bool) -> Markup {
    html!{
        tr {
            td  class="icon" { "⤴️" }
            td  class="link" {
                @if !top { a href=".." { ".." } }
            }
        }
    }
}
