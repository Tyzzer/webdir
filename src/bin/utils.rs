use std::path::PathBuf;
use std::fs::{ File, OpenOptions };
use std::str::FromStr;
use std::io::{ self, BufReader };
use rustls::{ Certificate, PrivateKey };
use rustls::internal::pemfile::{ certs, rsa_private_keys };
use slog::Logger;
use super::Config;


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

#[cfg(unix)]
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
            .map(|home| home.join(format!(".{}", CONFIG_NAME)))
            .and_then(|path|
                if path.is_file() { Some(path) }
                else { None }
            )
        )
}

#[cfg(not(unix))]
#[inline]
pub fn read_config() -> Option<PathBuf> {
    None
}


#[derive(Clone, Copy, Deserialize)]
pub enum Format {
    Compact,
    Full
}

impl Default for Format {
    fn default() -> Self {
        Format::Compact
    }
}

impl FromStr for Format {
    type Err = io::Error;

    fn from_str(input: &str) -> Result<Self, Self::Err> {
        match input.to_lowercase().as_str() {
            "compact" => Ok(Format::Compact),
            "full" => Ok(Format::Full),
            _ => Err(err!(Other, "parse format error"))
        }
    }
}

pub fn init_logging(config: &Config) -> io::Result<Logger> {
    use slog::{ Drain, Logger };
    use slog_term::{ CompactFormat, FullFormat, TermDecorator, PlainDecorator };
    use slog_async::Async;

    macro_rules! decorator {
        ( plain $path:expr ) => {{
            let file = OpenOptions::new()
                .create(true)
                .write(true)
                .truncate(true)
                .open($path)?;

            PlainDecorator::new(file)
        }};
        ( term ) => {
            TermDecorator::new().build()
        };
    }

    macro_rules! drain {
        ( choose $config:expr ) => {
            if let Some(ref path) = $config.log_output {
                drain!(format $config, decorator!(plain path))
            } else {
                drain!(format $config, decorator!(term))
            }
        };
        ( format $config:expr, $decorator:expr ) => {
            match $config.format.unwrap_or_default() {
                Format::Compact => drain!(compact $decorator),
                Format::Full => drain!(full $decorator)
            }
        };
        ( compact $decorator:expr ) => {
            drain!(async CompactFormat::new($decorator).build().fuse())
        };
        ( full $decorator:expr ) => {
            drain!(async FullFormat::new($decorator).build().fuse())
        };
        ( async $drain:expr ) => {
            Async::new($drain).build().fuse()
        }
    }


    let drain = drain!(choose config);
    Ok(Logger::root(drain, o!("version" => env!("CARGO_PKG_VERSION"))))
}
