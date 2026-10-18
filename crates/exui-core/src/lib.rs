#![no_std]

extern crate alloc;
#[cfg(test)]
extern crate std;

pub mod archive;
mod cursor;
pub mod custom;
pub mod error;
pub mod schema;
pub mod value;
pub mod wide;
pub mod xur;

#[cfg(test)]
mod tests;

pub use archive::Archive;
pub use error::ArchiveError;
pub use error::Region;
pub use error::RegisterError;
pub use error::XurError;
pub use schema::Registry;
pub use xur::Document;
