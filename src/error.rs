pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{0}")]
    Io(#[from] std::io::Error),
    #[error("{0}")]
    Ssh(String),
    #[error("could not find your home folder")]
    NoHome,
    #[error("use letters, numbers, dots, dashes or underscores for the name")]
    InvalidName,
    #[error("a key with that name already exists")]
    Exists,
    #[error("that file is not an OpenSSH key")]
    NotAKey,
    #[error("that key no longer exists")]
    NotFound,
    #[error("a host with that alias is already in your config")]
    HostExists,
    #[error("use a single word without spaces for the alias")]
    InvalidAlias,
}
