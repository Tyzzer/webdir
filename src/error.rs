use std::io;
#[cfg(feature = "sendfile")] use ::nix;
use ::futures::sync::mpsc::SendError;
use ::hyper;

#[derive(Debug, Fail)]
pub enum Error {
    #[fail(display = "IO Error {}", _0)]
    Io(io::Error),

    #[cfg(feature = "sendfile")]
    #[fail(display = "Nix Error {}", _0)]
    Nix(nix::Error),

    #[fail(display = "Send Error {}", _0)]
    SendError(SendError<hyper::Result<hyper::Chunk>>)
}

impl From<io::Error> for Error {
    fn from(err: io::Error) -> Self {
        Error::Io(err)
    }
}

#[cfg(feature = "sendfile")]
impl From<nix::Error> for Error {
    fn from(err: nix::Error) -> Self {
        Error::Nix(err)
    }
}

impl From<SendError<hyper::Result<hyper::Chunk>>> for Error {
    fn from(err: SendError<hyper::Result<hyper::Chunk>>) -> Self {
        Error::SendError(err)
    }
}
