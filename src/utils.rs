use std::mem;
use std::ops::Add;
use std::path::{ Path, PathBuf, Component };
use url::percent_encoding::{ DEFAULT_ENCODE_SET, percent_encode, percent_decode };


macro_rules! boundary {
    () => { env!("CARGO_PKG_NAME") }
}

#[macro_export]
macro_rules! err {
    ( os $number:expr ) => {
        ::std::io::Error::from_raw_os_error($number)
    };
    ( $kind:ident ) => {
        ::std::io::Error::from(
            ::std::io::ErrorKind::$kind
        )
    };
    ( $kind:ident, $err:expr ) => {
        ::std::io::Error::new(
            ::std::io::ErrorKind::$kind,
            $err
        )
    };
    ( $kind:ident, $fmt:expr, $( $args:tt )+ ) => {
        err!($kind, format!($fmt, $($args)+))
    }
}

macro_rules! chunk {
    ( into $chunk:expr ) => {
        Ok(::hyper::Chunk::from($chunk.into_string()))
    };
    ( $chunk:expr ) => {
        Ok(::hyper::Chunk::from($chunk))
    };
}

pub(crate) fn path_canonicalize<P: AsRef<Path>>(root: &Path, path: P) -> (usize, PathBuf) {
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


#[test]
fn test_path_canonicalize() {
    let root = Path::new("/home/");

    assert_eq!(
        path_canonicalize(&root, "../../../aaa.txt"),
        (1, PathBuf::from("/home/aaa.txt"))
    );

    assert_eq!(
        path_canonicalize(&root, "/aa/../../../aaa.txt"),
        (1, PathBuf::from("/home/aaa.txt"))
    );

    assert_eq!(
        path_canonicalize(&root, "/aa/../../../"),
        (0, PathBuf::from("/home/"))
    );

    assert_eq!(
        path_canonicalize(&root, "aaa/bbb/ccc/../../ddd/"),
        (2, PathBuf::from("/home/aaa/ddd/"))
    );

    assert_eq!(
        path_canonicalize(&root, "aaa/bbb/ccc/../../ddd/aaa.txt"),
        (3, PathBuf::from("/home/aaa/ddd/aaa.txt"))
    );
}


#[inline]
pub(crate) fn u64_to_bytes(x: u64) -> [u8; 8] {
    unsafe { mem::transmute(x) }
}

#[test]
fn test_u64_to_bytes() {
    assert_eq!(u64_to_bytes(0), [0; 8]);
    assert_eq!(u64_to_bytes(::std::u64::MAX), [255; 8]);
}


#[cfg(unix)]
#[inline]
pub(crate) fn encode_path(path: &Path) -> String {
    use std::os::unix::ffi::OsStrExt;

    percent_encode(path.as_os_str().as_bytes(), DEFAULT_ENCODE_SET)
        .fold(String::from("/"), Add::add)
}

#[cfg(not(unix))]
#[inline]
pub(crate) fn encode_path(path: &Path) -> String {
    percent_encode(path.to_string_lossy().as_bytes(), DEFAULT_ENCODE_SET)
        .fold(String::from("/"), Add::add)
}


#[cfg(unix)]
#[inline]
pub(crate) fn decode_path(path: &str) -> PathBuf {
    use std::ffi::OsString;
    use std::os::unix::ffi::OsStringExt;

    let path_buf = percent_decode(path.as_bytes()).collect::<Vec<u8>>();
    let path_buf = OsString::from_vec(path_buf);
    PathBuf::from(path_buf)
}

#[cfg(not(unix))]
#[inline]
pub(crate) fn decode_path(path: &str) -> PathBuf {
    let path_buf = percent_decode(path.as_bytes()).collect::<Vec<u8>>();
    let path_buf = String::from_utf8_lossy(&path_buf).into_owned();
    PathBuf::from(path_buf)
}

#[test]
fn test_encode_path() {
    assert_eq!(encode_path(Path::new("aaa/bbb")), "/aaa/bbb");
    assert_eq!(encode_path(Path::new("aaa/中文")), "/aaa/%E4%B8%AD%E6%96%87");
}

#[test]
fn test_decode_path() {
    assert_eq!(decode_path("aaa/bbb"), Path::new("aaa/bbb"));
    assert_eq!(decode_path("%E4%B8%AD%E6%96%87/bbb"), Path::new("中文/bbb"));
}
