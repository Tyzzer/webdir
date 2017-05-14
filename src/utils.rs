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


pub fn path_canonicalize<P: AsRef<Path>>(root: &Path, path: P) -> PathBuf {
    path.as_ref()
        .components()
        .fold(root.to_path_buf(), |mut sum, next| {
            match next {
                Component::Normal(p) => sum.push(p),
                Component::ParentDir => { sum.pop(); },
                _ => ()
            };
            sum
        })
}


#[test]
fn test_path_canonicalize() {
    assert_eq!(
        path_canonicalize("../../../aaa.txt"),
        Path::new("aaa.txt")
    );

    assert_eq!(
        path_canonicalize("/aa/../../../aaa.txt"),
        Path::new("aaa.txt")
    );

    assert_eq!(
        path_canonicalize("aaa/bbb/ccc/../../ddd/aaa.txt"),
        Path::new("aaa/ddd/aaa.txt")
    );
}
