use std::cmp;
use std::path::Path;
use std::fs::Metadata;
use std::hash::Hasher;
use std::ops::Range;
use std::os::unix::fs::MetadataExt;
use hyper::{ Headers, Response, StatusCode };
use hyper::header::{ self, ByteRangeSpec };
use smallvec::SmallVec;
use mime_guess::guess_mime_type;
use metrohash::MetroHash;
use data_encoding::base64url;
use slog::Logger;
use ::{ resp, utils };


pub fn process_range(log: &Logger, ranges: &[ByteRangeSpec], length: u64) -> Result<SmallVec<[Range<u64>; 1]>, Response> {
    let mut vec = SmallVec::new();

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
        Err(resp::fail(log, false, StatusCode::RangeNotSatisfiable, &err!(Other, "Bad Range"))
            .with_header(header::ContentRange(header::ContentRangeSpec::Bytes {
                range: None, instance_length: Some(length)
            }))
        )
    } else {
        Ok(vec)
    }
}


pub fn etag(metadata: &Metadata) -> header::EntityTag {
    let mut hasher = MetroHash::default();
    hasher.write_u64(metadata.ino());
    hasher.write_u64(metadata.len());
    hasher.write_i64(metadata.mtime());
    hasher.write_i64(metadata.mtime_nsec());
    header::EntityTag::strong(
        base64url::encode_nopad(&utils::u64_to_bytes(hasher.finish()))
    )
}

pub fn resp_headers(path: &Path, metadata: &Metadata) -> Headers {
    let mut headers = Headers::new();
    headers.set(header::ContentType(guess_mime_type(&path)));
    headers.set(header::ContentLength(metadata.len()));
    headers.set(header::AcceptRanges(vec![header::RangeUnit::Bytes]));

    if let Ok(date) = metadata.modified() {
        headers.set(header::LastModified(header::HttpDate::from(date)));
    }

    headers
}
