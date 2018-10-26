use std::{ fmt, cmp };
use std::ops::{ Bound, Range };
use std::path::Path;
use std::fs::Metadata;
use std::str::FromStr;
use smallvec::SmallVec;
use rand::{ Rng, thread_rng, distributions::Alphanumeric };
use log::{ log, info, debug };
use hyper::{ StatusCode, Body };
use headers_core::HeaderMapExt;
use headers_core::header::HeaderMap;
use headers_ext as header;
use mime::Mime;
use mime_guess::guess_mime_type;
use crate::common::err_html;


pub struct Entity<'a> {
    pub path: &'a Path,
    pub length: u64,
    metadata: &'a Metadata
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
        Entity {
            path, metadata,
            length: metadata.len()
        }
    }

    pub fn headers(&self) -> HeaderMap {
        let mut map = HeaderMap::new();

        map.typed_insert(header::AcceptRanges::bytes());
        map.typed_insert(header::ContentType::from(guess_mime_type(self.path)));

        if let Ok(date) = self.metadata.modified() {
            map.typed_insert(header::LastModified::from(date));
        }

        map
    }

    pub fn multipart_headers(&self, boundary: &str) -> HeaderMap {
        let mut map = HeaderMap::new();

        map.typed_insert(header::AcceptRanges::bytes());

        // TODO https://github.com/hyperium/mime/issues/52
        let mime = Mime::from_str(format!("multipart/byteranges; boundary={}", boundary).as_str()).unwrap();
        map.typed_insert(header::ContentType::from(mime));

        if let Ok(date) = self.metadata.modified() {
            map.typed_insert(header::LastModified::from(date));
        }

        map
    }

    pub fn result(&self, map: &HeaderMap) -> Result {
        // TODO check etag

        if let Some(time) = map.typed_get::<header::IfModifiedSince>() {
            if let Ok(time2) = self.metadata.modified() {
                if !time.is_modified(time2) {
                    return not_modified(format_args!("{:?} vs {:?}", time, time2));
                }
            }
        }

        if let Some(ranges) = map.typed_get::<header::Range>() {
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
                map.typed_insert(header::ContentRange::unsatisfied_bytes(length));
                Result(
                    StatusCode::RANGE_NOT_SATISFIABLE,
                    map,
                    Value::Error(Body::from("Bad Range"))
                )
            } else if vec.len() == 1 {
                let mut map = self.headers();
                let range = &vec[0];
                map.typed_insert(header::ContentLength(range.end - range.start));
                map.typed_insert(header::ContentRange::bytes(range.start, range.end - 1, length));
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
            map.typed_insert(header::ContentLength(self.length));
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
