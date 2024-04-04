use std::{ fmt, fs, io };
use std::ffi::OsStr;
use std::ops::Add;
use std::hash::Hasher;
use std::path::{ Path, PathBuf, Component };
use bytes::Bytes;
use siphasher::sip::SipHasher;
use percent_encoding::{ NON_ALPHANUMERIC, percent_encode, percent_decode };
use maud::{ html, Markup };
use crate::file::File;


pub fn html_utf8() -> headers::ContentType {
    headers::ContentType::from(mime::TEXT_HTML_UTF_8)
}

pub fn err_html(display: fmt::Arguments) -> Markup {
    html!{
        h1 { strong { "( ・_・)" } }
        h2 { (display) }
    }
}

pub fn path_canonicalize<P: AsRef<Path>>(root: &Path, path: P) -> (usize, PathBuf) {
    path.as_ref()
        .components()
        .fold((0, root.to_path_buf()), |(mut depth, mut sum), next| {
            match next {
                Component::Normal(p) => {
                    sum.push(p);
                    depth += 1;
                },
                Component::ParentDir if depth > 0 => if sum.pop() {
                    depth -= 1;
                },
                _ => ()
            };
            (depth, sum)
        })
}


#[cfg(unix)]
#[inline]
pub fn encode_path(name: &OsStr) -> String {
    use std::os::unix::ffi::OsStrExt;

    let mut init = String::with_capacity(name.len() + 2);
    init.push_str("./");

    percent_encode(name.as_bytes(), NON_ALPHANUMERIC)
        .fold(init, Add::add)
}

#[cfg(not(unix))]
#[inline]
pub fn encode_path(name: &OsStr) -> String {
    let mut init = String::with_capacity(name.len() + 2);
    init.push_str("./");

    percent_encode(name.to_string_lossy().as_bytes(), NON_ALPHANUMERIC)
        .fold(init, Add::add)
}

#[cfg(unix)]
#[inline]
pub fn decode_path(path: &str) -> PathBuf {
    use std::ffi::OsString;
    use std::os::unix::ffi::OsStringExt;

    let path_buf = percent_decode(path.as_bytes()).collect::<Vec<u8>>();
    let path_buf = OsString::from_vec(path_buf);
    PathBuf::from(path_buf)
}

#[cfg(not(unix))]
#[inline]
pub fn decode_path(path: &str) -> PathBuf {
    let path_buf = percent_decode(path.as_bytes())
        .decode_utf8_lossy()
        .into_owned();
    PathBuf::from(path_buf)
}


#[cfg(unix)]
pub fn fs_hash(metadata: &fs::Metadata) -> u64 {
    use std::os::unix::fs::MetadataExt;

    let mut hasher = SipHasher::default();
    hasher.write_u64(metadata.size());
    hasher.write_u64(metadata.ino());
    hasher.write_i64(metadata.ctime());
    hasher.write_i64(metadata.ctime_nsec());
    hasher.write_i64(metadata.mtime());
    hasher.write_i64(metadata.mtime_nsec());
    hasher.finish()
}

#[cfg(windows)]
pub fn fs_hash(metadata: &fs::Metadata) -> u64 {
    use std::os::windows::fs::MetadataExt;

    let mut hasher = SipHasher::default();
    hasher.write_u32(metadata.file_attributes());
    hasher.write_u64(metadata.creation_time());
    hasher.write_u64(metadata.last_write_time());
    hasher.write_u64(metadata.file_size());
    hasher.finish()
}

pub struct LimitFile<'a> {
    inner: &'a mut File,
    curr: u64,
    limit: u64
}

impl LimitFile<'_> {
    pub fn new(fd: &mut File, limit: u64) -> LimitFile<'_> {
        LimitFile {
            inner: fd,
            curr: 0,
            limit
        }
    }

    pub async fn next_chunk(&mut self) -> io::Result<Option<Bytes>> {
        Ok(if let Some(buf) = self.inner.next_chunk().await? {
            Some(if self.curr + (buf.len() as u64) > self.limit {
                let len = self.limit - self.curr;
                let buf = buf.slice(..len as usize);
                self.curr += len;
                buf
            } else {
                self.curr += buf.len() as u64;
                buf
            })
        } else {
            None
        })
    }
}
