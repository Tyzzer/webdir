use std::{ fmt, cmp };
use std::ops::{ Bound, Range };
use std::path::Path;
use std::fs::Metadata;
use std::str::FromStr;
use std::cell::RefCell;
use smallvec::SmallVec;
use rand::{ Rng, thread_rng, distributions::Alphanumeric };
use log::*;
use hyper::{ StatusCode, Body };
use http::HeaderMap;
use headers::HeaderMapExt;
use mime::Mime;
use data_encoding::BASE64URL_NOPAD;
use crate::common::{ err_html, fs_hash };


pub struct Entity<'a> {
    pub path: &'a Path,
    pub length: u64,
    metadata: &'a Metadata,
    etag: headers::ETag
}

pub struct Result(pub StatusCode, pub HeaderMap, pub Value);

pub enum Value {
    Error(Body),
    None,
    Range(Range<u64>),
    Multipart(String, Vec<Range<u64>>)
}

impl<'a> Entity<'a> {
    pub fn new(path: &'a Path, metadata: &'a Metadata) -> Self {
        thread_local!{
            static BUF: RefCell<String> = RefCell::new(String::with_capacity(16));
        }

        let hash = fs_hash(metadata);

        let etag = BUF.with(|buf| {
            let mut buf = buf.borrow_mut();

            buf.clear();
            buf.push('"');
            BASE64URL_NOPAD.encode_append(&hash.to_le_bytes(), &mut buf);
            buf.push('"');

            buf.parse().unwrap()
        });

        Entity {
            path, metadata, etag,
            length: metadata.len()
        }
    }

    pub fn headers(&self) -> HeaderMap {
        let mut map = HeaderMap::new();

        map.typed_insert(headers::AcceptRanges::bytes());
        let mime = mime_guess::from_path(self.path).first_or_octet_stream();
        map.typed_insert(headers::ContentType::from(mime));

        map.typed_insert(self.etag.clone());

        if let Ok(date) = self.metadata.modified() {
            map.typed_insert(headers::LastModified::from(date));
        }

        map
    }

    pub fn multipart_headers(&self, boundary: &str) -> HeaderMap {
        let mut map = HeaderMap::new();

        map.typed_insert(headers::AcceptRanges::bytes());

        // TODO https://github.com/hyperium/mime/issues/52
        let mime = Mime::from_str(format!("multipart/byteranges; boundary={}", boundary).as_str()).unwrap();
        map.typed_insert(headers::ContentType::from(mime));

        if let Ok(date) = self.metadata.modified() {
            map.typed_insert(headers::LastModified::from(date));
        }

        map
    }

    pub fn result(&self, map: &HeaderMap) -> Result {
        if let Some(ifmatch) = map.typed_get::<headers::IfMatch>() {
            if !ifmatch.precondition_passes(&self.etag) {
                return Result(
                    StatusCode::PRECONDITION_FAILED,
                    HeaderMap::new(),
                    Value::Error(Body::from("Precondition failed"))
                );
            }
        }

        if let Some(ifrange) = map.typed_get::<headers::IfRange>() {
            let mtime = self.metadata.modified()
                .map(headers::LastModified::from)
                .ok();
            if ifrange.is_modified(Some(&self.etag), mtime.as_ref()) {
                return Result(
                    StatusCode::PRECONDITION_FAILED,
                    HeaderMap::new(),
                    Value::Error(Body::from("Precondition failed"))
                );
            }
        }

        if let Some(ifnonematch) = map.typed_get::<headers::IfNoneMatch>() {
            if !ifnonematch.precondition_passes(&self.etag) {
                return not_modified(format_args!("etag: {:?}", &self.etag));
            }
        }

        if let Some(time) = map.typed_get::<headers::IfModifiedSince>() {
            if let Ok(time2) = self.metadata.modified() {
                if !time.is_modified(time2) {
                    return not_modified(format_args!("{:?} vs {:?}", time, time2));
                }
            }
        }

        if let Some(ranges) = map.typed_get::<headers::Range>() {
            let length = self.length;

            let mut vec = ranges
                .iter()
                .filter_map(|(start, end)| {
                    let start = match start {
                        Bound::Excluded(x) | Bound::Included(x) => x,
                        Bound::Unbounded => 0
                    };

                    let end = match end {
                        Bound::Excluded(y) => y,
                        Bound::Included(y) => y + 1,
                        Bound::Unbounded => length,
                    };
                    let end = cmp::min(end, length);

                    if start <= end {
                        Some(start..end)
                    } else {
                        None
                    }
                })
                .collect::<SmallVec<[_; 1]>>();

            if vec.is_empty() {
                let mut map = self.headers();
                map.typed_insert(headers::ContentRange::unsatisfied_bytes(length));
                Result(
                    StatusCode::RANGE_NOT_SATISFIABLE,
                    map,
                    Value::Error(Body::from("Bad Range"))
                )
            } else if vec.len() == 1 {
                let mut map = self.headers();
                let range = &vec[0];
                map.typed_insert(headers::ContentLength(range.end - range.start));
                map.typed_insert(headers::ContentRange::bytes(range.clone(), length).unwrap());
                Result(StatusCode::PARTIAL_CONTENT, map, Value::Range(vec.pop().unwrap()))
            } else {
                let boundary = thread_rng()
                    .sample_iter(&Alphanumeric)
                    .take(12)
                    .collect::<String>();
                let map = self.multipart_headers(&boundary);
                Result(StatusCode::PARTIAL_CONTENT, map, Value::Multipart(boundary, vec.into_vec()))
            }
        } else {
            let mut map = self.headers();
            map.typed_insert(headers::ContentLength(self.length));
            Result(StatusCode::OK, map, Value::None)
        }
    }
}


pub fn not_modified(display: fmt::Arguments) -> Result {
    debug!("send/cache: {}", display);

    let map = HeaderMap::new();
    let body = err_html(format_args!("Not Modified: {}", display)).into_string();
    Result(StatusCode::NOT_MODIFIED, map, Value::Error(Body::from(body)))
}
