use std::path::PathBuf;
use std::fs::File;
use std::io::{ self, BufReader };
use rustls::{ Certificate, PrivateKey };
use rustls::internal::pemfile::{ certs, rsa_private_keys };


#[inline]
pub fn load_certs(path: &str) -> io::Result<Vec<Certificate>> {
    certs(&mut BufReader::new(File::open(path)?))
        .map_err(|_| err!(Other, "Not found cert"))
}

#[inline]
pub fn load_keys(path: &str) -> io::Result<Vec<PrivateKey>> {
    rsa_private_keys(&mut BufReader::new(File::open(path)?))
        .map_err(|_| err!(Other, "Not found keys"))
}

#[inline]
pub fn read_config() -> Option<PathBuf> {
    use std::env;
    use xdg::BaseDirectories;
    const CONFIG_NAME: &str = concat!(env!("CARGO_PKG_NAME"), ".toml");

    BaseDirectories::with_prefix(env!("CARGO_PKG_NAME"))
        .ok()
        .and_then(|xdg| xdg.find_config_file(CONFIG_NAME))
        .or_else(|| BaseDirectories::new()
            .ok()
            .and_then(|xdg| xdg.find_config_file(CONFIG_NAME))
        )
        .or_else(|| env::home_dir()
            .map(|home| home.join(CONFIG_NAME))
            .and_then(|path|
                if path.is_file() { Some(path) }
                else { None }
            )
        )
}
