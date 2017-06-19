use std::{ io, cmp, fs };
use std::rc::Rc;
use std::ops::Range;
use std::hash::Hasher;
use std::path::PathBuf;
use std::fs::Metadata;
use tokio_core::reactor::Handle;
use hyper::{ header, Headers, Response, StatusCode };
use hyper::header::ByteRangeSpec;
use slog::Logger;
use smallvec::SmallVec;
use mime_guess::guess_mime_type;
use metrohash::MetroHash;
use data_encoding::base64url;
use ::response::{ BOUNDARY, fail, not_modified };
use ::utils::u64_to_bytes;
use ::file;


pub struct Entity<'a> {
    path: Rc<PathBuf>,
    metadata: &'a Metadata,
    log: &'a Logger,
    len: u64,
    etag: header::EntityTag
}

pub enum EntifyResult {
    Err(Response),
    None,
    One(Range<u64>),
    Vec(Vec<Range<u64>>)
}

impl<'a> Entity<'a> {
    pub fn new(path: Rc<PathBuf>, metadata: &'a Metadata, log: &'a Logger) -> Self {
        let len = metadata.len();
        Entity { path, metadata, log, len, etag: Self::etag(len, metadata) }
    }

    #[cfg(unix)]
    fn etag(len: u64, metadata: &Metadata) -> header::EntityTag {
        use std::os::unix::fs::MetadataExt;

        let mut hasher = MetroHash::default();
        hasher.write_u64(len);
        hasher.write_u64(metadata.ino());
        hasher.write_i64(metadata.mtime());
        hasher.write_i64(metadata.mtime_nsec());
        header::EntityTag::strong(
            base64url::encode_nopad(&u64_to_bytes(hasher.finish()))
        )
    }

    #[cfg(not(unix))]
    fn etag(len: u64, metadata: &Metadata) -> header::EntityTag {
        use std::hash::Hash;
        use std::time::UNIX_EPOCH;

        let mut hasher = MetroHash::default();
        metadata.file_type().hash(&mut hasher);
        len.hash(&mut hasher);

        if let Ok(time) = metadata.created() {
            if let Ok(time) = time.duration_since(UNIX_EPOCH) {
                time.hash(&mut hasher);
            }
        }

        if let Ok(time) = metadata.modified() {
            if let Ok(time) = time.duration_since(UNIX_EPOCH) {
                time.hash(&mut hasher);
            }
        }

        header::EntityTag::strong(
            base64url::encode_nopad(&u64_to_bytes(hasher.finish()))
        )
    }

    #[inline]
    pub fn open(&self, handle: Handle) -> io::Result<file::File> {
        let fd = fs::File::open(&*self.path)?;
        file::File::new(fd, handle, self.len)
    }

    pub fn headers(self, is_multipart: bool) -> Headers {
        let mut headers = Headers::new();

        headers.set(header::AcceptRanges(vec![header::RangeUnit::Bytes]));
        headers.set(header::ETag(self.etag));

        if is_multipart {
            // TODO https://github.com/hyperium/mime/issues/52
            let mime = format!("multipart/byteranges; boundary={}", BOUNDARY).parse().unwrap();
            headers.set(header::ContentType(mime));
        } else {
            headers.set(header::ContentType(guess_mime_type(&*self.path)));
        }

        if let Ok(date) = self.metadata.modified() {
            headers.set(header::LastModified(header::HttpDate::from(date)));
        }

        headers
    }

    pub fn check(&self, headers: &Headers) -> EntifyResult {
        if let Some(&header::IfMatch::Items(ref etags)) = headers.get::<header::IfMatch>() {
            if !etags.iter().any(|e| self.etag.strong_eq(e)) {
                return EntifyResult::Err(fail(
                    self.log, false, StatusCode::PreconditionFailed,
                    &err!(Other, "Precondition failed")
                ));
            }
        }

        if let Some(&header::IfNoneMatch::Items(ref etags)) = headers.get::<header::IfNoneMatch>() {
            if etags.iter().any(|e| self.etag.weak_eq(e)) {
                return EntifyResult::Err(not_modified(self.log, format_args!("{}", self.etag)));
            }
        }

        if let Some(&header::IfModifiedSince(ref date)) = headers.get::<header::IfModifiedSince>() {
            if let Ok(ndate) = self.metadata.modified() {
                if date >= &header::HttpDate::from(ndate) {
                    return EntifyResult::Err(not_modified(self.log, format_args!("{}", date)));
                }
            }
        }

        if let Some(&header::Range::Bytes(ref ranges)) = headers.get::<header::Range>() {
            let length = self.len;
            let mut vec = SmallVec::<[_; 1]>::new();

            for range in ranges {
                match *range {
                    ByteRangeSpec::FromTo(x, y) => {
                        let y = cmp::min(y + 1, length);
                        if x < y {
                            vec.push(x..y);
                        }
                    },
                    ByteRangeSpec::AllFrom(x) if x < length => vec.push(x..length),
                    ByteRangeSpec::Last(y) if y < length => vec.push(length - y..length),
                    _ => ()
                }
            }

            if vec.is_empty() {
                EntifyResult::Err(fail(self.log, false, StatusCode::RangeNotSatisfiable, &err!(Other, "Bad Range"))
                    .with_header(header::ContentRange(header::ContentRangeSpec::Bytes {
                        range: None, instance_length: Some(length)
                    }))
                )
            } else if vec.len() == 1 {
                EntifyResult::One(vec.pop().unwrap())
            } else {
                EntifyResult::Vec(vec.into_vec())
            }
        } else {
            EntifyResult::None
        }
    }
}
