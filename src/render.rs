use std::io;
use std::ops::Add;
use std::borrow::Cow;
use std::time::UNIX_EPOCH;
use std::fs::{ DirEntry, Metadata };
use std::path::{ PathBuf, Path, StripPrefixError };
use std::os::unix::ffi::OsStrExt;
use url::percent_encoding;
use maud::{ Render, Markup, PreEscaped };
use chrono::{ TimeZone, UTC };


pub struct Entry {
    pub metadata: Metadata,
    pub path: PathBuf,
    pub uri: Option<String>
}

impl Entry {
    pub fn new(base: &Path, entry: DirEntry) -> io::Result<Self> {
        let metadata = entry.metadata()?;
        let path = entry.path();
        let uri = path.strip_prefix(base)
            .map(|p| percent_encoding::percent_encode(
                p.as_os_str().as_bytes(),
                percent_encoding::PATH_SEGMENT_ENCODE_SET
            ))
            .map(|p| p.fold(String::new(), Add::add))
            .ok();

        Ok(Entry { metadata, path, uri })
    }

    pub fn name(&self) -> Cow<str> {
        self.path
            .file_name()
            .map(|p| p.to_string_lossy())
            .unwrap_or(Cow::Borrowed(".."))
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
        let file_type = self.metadata.file_type();

        html!{
            tr {
                td class="icon" @if file_type.is_dir() {
                    "📁"
                } @else if file_type.is_file() {
                    "📄"
                } @else {
                    "↩️"
                }

                td class="link" @if let Some(ref uri) = self.uri {
                    a href=(uri) (self.name())
                } @else {
                    (self.name())
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
            td          class="icon"    "⤴️"
            td          class="link"    @if !top { a href=".." ".." }
            td small    class="time"    "-"
            td          class="size"    "-"
        }
    }
}
