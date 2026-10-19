#![no_std]

extern crate alloc;
#[cfg(test)]
extern crate std;

pub mod archive;
pub mod archive_writer;
mod cursor;
pub mod custom;
pub mod error;
pub mod schema;
pub mod tree;
pub mod value;
pub mod wide;
pub mod writer;
pub mod xur;

#[cfg(test)]
mod tests;

pub use archive::AcceptedVersions;
pub use archive::Archive;
pub use archive::ArchiveVersion;
pub use error::ArchiveError;
pub use error::ArchiveWriteError;
pub use error::Region;
pub use error::RegisterError;
pub use error::TreeError;
pub use error::WriteError;
pub use error::XurError;
pub use schema::Registry;
pub use xur::Document;
