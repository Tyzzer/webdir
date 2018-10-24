use std::io;
use std::ffi::OsStr;
use std::ops::Add;
use std::path::{ Path, PathBuf, Component };
use percent_encoding::{ DEFAULT_ENCODE_SET, percent_encode, percent_decode };


pub fn err(e: hyper::Error) -> io::Error {
    io::Error::new(
        if e.is_parse() {
            io::ErrorKind::InvalidData
        } else if e.is_canceled() {
            io::ErrorKind::Interrupted
        } else if e.is_closed() {
            io::ErrorKind::ConnectionAborted
        } else if e.is_user() {
            io::ErrorKind::InvalidInput
        } else {
            io::ErrorKind::Other
        },
        e
    )
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

    percent_encode(name.as_bytes(), DEFAULT_ENCODE_SET)
        .fold(String::from("./"), Add::add)
}

#[cfg(not(unix))]
#[inline]
pub fn encode_path(name: &OsStr) -> String {
    percent_encode(name.to_string_lossy().as_bytes(), DEFAULT_ENCODE_SET)
        .fold(String::from("./"), Add::add)
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
    let path_buf = percent_decode(path.as_bytes()).collect::<Vec<u8>>();
    let path_buf = String::from_utf8_lossy(&path_buf).into_owned();
    PathBuf::from(path_buf)
}
