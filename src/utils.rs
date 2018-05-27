use std::ops::Add;
use std::ffi::OsStr;
use std::path::{ Path, PathBuf, Component };
use percent_encoding::{ DEFAULT_ENCODE_SET, percent_encode, percent_decode };


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

macro_rules! chain {
    ( @parse + $stream:expr ) => {
        $stream
    };
    ( @parse $chunk:expr ) => {
        ::futures::stream::once(Ok(Ok(::hyper::Chunk::from($chunk))))
    };
    (
        type Item = $item:ty;
        type Error = $err:ty;
        $( ( $( $stream:tt )* ) ),*
    ) => {
        ::futures::stream::empty::<$item, $err>()
        $(
            .chain(chain!(@parse $( $stream )*))
        )*
    };
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


#[inline]
pub fn u64_to_bytes(x: u64) -> [u8; 8] {
    use byteorder::{ ByteOrder, LittleEndian };

    let mut buf = [0; 8];
    LittleEndian::write_u64(&mut buf, x);
    buf
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

#[cfg(test)]
mod test {
    use super::*;

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

    #[test]
    fn test_u64_to_bytes() {
        assert_eq!(u64_to_bytes(0), [0; 8]);
        assert_eq!(u64_to_bytes(::std::u64::MAX), [255; 8]);
    }

    #[test]
    fn test_encode_path() {
        assert_eq!(encode_path(OsStr::new("aaa")), "./aaa");
        assert_eq!(encode_path(OsStr::new("中文")), "./%E4%B8%AD%E6%96%87");
    }

    #[test]
    fn test_decode_path() {
        assert_eq!(decode_path("aaa"), OsStr::new("aaa"));
        assert_eq!(decode_path("%E4%B8%AD%E6%96%87"), OsStr::new("中文"));
    }
}
