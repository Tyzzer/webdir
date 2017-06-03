use std::mem;
use std::path::{ Path, PathBuf, Component };


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
    ( ok $chunk:expr ) => {
        Ok(chunk!($chunk))
    };

    ( $chunk:expr ) => {
        ::hyper::Chunk::from($chunk.into_string())
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
pub fn u64_to_bytes(x: u64) -> [u8; 8] {
    unsafe { mem::transmute(x) }
}
