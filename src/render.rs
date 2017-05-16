use std::io;
use std::ops::Add;
use std::path::Path;
use std::time::UNIX_EPOCH;
use std::fs::{ DirEntry, Metadata };
use std::os::unix::ffi::OsStrExt;
use url::percent_encoding;
use maud::{ Render, Markup };
use chrono::{ TimeZone, UTC };


pub struct Entry {
    pub metadata: Metadata,
    pub name: String,
    pub uri: Option<String>,
    pub is_symlink: bool
}

impl Entry {
    pub fn new(base: &Path, entry: DirEntry) -> io::Result<Self> {
        let mut metadata = entry.metadata()?;
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().into();
        let is_symlink = metadata.file_type().is_symlink();
        if is_symlink {
            metadata = path.metadata()?;
        }

        let uri = path.strip_prefix(base)
            .map(|p| percent_encoding::percent_encode(
                p.as_os_str().as_bytes(),
                percent_encoding::DEFAULT_ENCODE_SET
            ))
            .map(|p| p.fold(String::from("/"), Add::add))
            .map(|p| if metadata.is_dir() { p + "/" } else { p })
            .ok();

        Ok(Entry { metadata, name, uri, is_symlink })
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

    #[inline]
    pub fn ty(&self) -> u8 {
        let ty = self.metadata.file_type();
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
}

impl Render for Entry {
    fn render(&self) -> Markup {
        let file_type = self.metadata.file_type();

        html!{
            tr {
                td class="icon" @if self.is_symlink {
                    "↩️"
                } @else if file_type.is_file() {
                    "📄"
                } @else if file_type.is_dir() {
                    "📁"
                } @else {
                    "❓"
                }

                td class="link" @if let Some(ref uri) = self.uri {
                    a href=(uri) (self.name)
                } @else {
                    (self.name)
                }

                td small class="time" @if let Ok(time) = self.time() {
                    (time)
                } @else {
                    "-"
                }

                td class="size" @if file_type.is_file() {
                    (self.size())
                } @else {
                    "-"
                }
            }
        }
    }
}

pub fn up(top: bool) -> Markup {
    html!{
        tr {
            td  class="icon"    "⤴️"
            td  class="link"    @if !top { a href=".." ".." }
        }
    }
}
