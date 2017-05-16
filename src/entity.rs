use std::path::Path;
use std::fs::Metadata;
use std::hash::Hasher;
use std::os::unix::fs::MetadataExt;
use hyper::{ Headers, Response, StatusCode };
use hyper::header::{ self, ByteRangeSpec };
use mime_guess::guess_mime_type;
use metrohash::MetroHash;
use data_encoding::base64url;
use slog::Logger;
use ::{ pages, utils };


pub fn check_range(log: &Logger, range: &ByteRangeSpec, length: u64) -> Result<(), Response> {
    match *range {
        ByteRangeSpec::FromTo(x, y) if x <= y && y <= length => Ok(()),
        ByteRangeSpec::AllFrom(x) if x <= length => Ok(()),
        ByteRangeSpec::Last(y) if y <= length => Ok(()),
        _ => Err(pages::fail(log, false, StatusCode::RangeNotSatisfiable, &err!(Other, "Bad Range"))
            .with_header(header::ContentRange(header::ContentRangeSpec::Bytes {
                range: None, instance_length: Some(length)
            })))
    }
}

pub fn resp_headers(path: &Path, metadata: &Metadata) -> Headers {
    let mut headers = Headers::new();
    headers.set(header::ContentType(guess_mime_type(&path)));
    headers.set(header::ContentLength(metadata.len()));
    headers.set(header::AcceptRanges(vec![header::RangeUnit::Bytes]));

    if let Ok(date) = metadata.modified() {
        headers.set(header::LastModified(header::HttpDate::from(date)));
    }

    let mut hasher = MetroHash::default();
    hasher.write_u64(metadata.ino());
    hasher.write_u64(metadata.len());
    hasher.write_i64(metadata.mtime());
    hasher.write_i64(metadata.mtime_nsec());
    headers.set(header::ETag(header::EntityTag::strong(
        base64url::encode_nopad(&utils::u64_to_bytes(hasher.finish()))
    )));

    headers
}
