use std::cmp;
use std::ops::Range;
use std::hash::Hasher;
use std::path::Path;
use std::fs::Metadata;
use std::os::unix::fs::MetadataExt;
use hyper::{ header, Headers, Response, StatusCode };
use hyper::header::ByteRangeSpec;
use slog::Logger;
use smallvec::SmallVec;
use mime_guess::guess_mime_type;
use metrohash::MetroHash;
use data_encoding::base64url;
use ::response::{ fail, not_modified };
use ::utils::u64_to_bytes;


pub struct Entity<'a> {
    path: &'a Path,
    metadata: &'a Metadata,
    log: &'a Logger,
    etag: header::EntityTag
}

pub enum EntifyResult {
    Err(Response),
    None,
    One(Range<u64>),
    Vec(Vec<Range<u64>>)
}

impl<'a> Entity<'a> {
    pub fn new(path: &'a Path, metadata: &'a Metadata, log: &'a Logger) -> Self {
        Entity { path, metadata, log, etag: Self::etag(metadata) }
    }

    fn etag(metadata: &Metadata) -> header::EntityTag {
        let mut hasher = MetroHash::default();
        hasher.write_u64(metadata.ino());
        hasher.write_u64(metadata.len());
        hasher.write_i64(metadata.mtime());
        hasher.write_i64(metadata.mtime_nsec());
        header::EntityTag::strong(
            base64url::encode_nopad(&u64_to_bytes(hasher.finish()))
        )
    }

    pub fn headers(&self) -> Headers {
        let mut headers = Headers::new();
        headers.set(header::ContentType(guess_mime_type(&self.path)));
        headers.set(header::ContentLength(self.metadata.len()));
        headers.set(header::AcceptRanges(vec![header::RangeUnit::Bytes]));

        if let Ok(date) = self.metadata.modified() {
            headers.set(header::LastModified(header::HttpDate::from(date)));
        }

        headers
    }

    pub fn check(&self, headers: &Headers) -> EntifyResult {
        if let Some(&header::IfMatch::Items(ref etags)) = headers.get::<header::IfMatch>() {
            if etags.iter().find(|e| self.etag.strong_eq(e)).is_none() {
                return EntifyResult::Err(fail(
                    self.log, false,
                    StatusCode::PreconditionFailed, &err!(Other, "Precondition failed")
                ));
            }
        }

        if let Some(&header::IfNoneMatch::Items(ref etags)) = headers.get::<header::IfNoneMatch>() {
            if etags.iter().any(|e| self.etag.weak_eq(e)) {
                return EntifyResult::Err(
                    not_modified(self.log, format_args!("{}", self.etag))
                        .with_headers(self.headers())
                );
            }
        }

        if let Some(&header::IfModifiedSince(ref date)) = headers.get::<header::IfModifiedSince>() {
            if let Ok(ndate) = self.metadata.modified() {
                if date >= &header::HttpDate::from(ndate) {
                    return EntifyResult::Err(
                        not_modified(self.log, format_args!("{}", date))
                            .with_headers(self.headers())
                    );
                }
            }
        }

        if let Some(&header::Range::Bytes(ref ranges)) = headers.get::<header::Range>() {
            let length = self.metadata.len();
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
                EntifyResult::One(vec.remove(0))
            } else {
                EntifyResult::Vec(vec.into_iter().collect())
            }
        } else {
            EntifyResult::None
        }
    }
}
