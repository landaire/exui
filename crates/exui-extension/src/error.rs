#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum EncodeError {
    #[error("a string is longer than 65535 bytes")]
    StringTooLong,

    #[error("a list holds more than 65535 items")]
    ListTooLong,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum DecodeError {
    #[error("descriptor magic is not OBXT")]
    Magic,

    #[error("descriptor version {version} is not supported")]
    Version { version: u16 },

    #[error("descriptor ends early or holds a malformed field at offset {offset:#x}")]
    Malformed { offset: usize },

    #[error("descriptor name is not a valid extension name")]
    Name,

    #[error("descriptor has {trailing} bytes after its last field")]
    Trailing { trailing: usize },
}
