error_chain!{
    foreign_links {
        Io(::std::io::Error);
        Nix(::nix::Error) #[cfg(feature = "sendfile")];
        SendError(::futures::sync::mpsc::SendError<::hyper::Result<::hyper::Chunk>>);
    }
}
