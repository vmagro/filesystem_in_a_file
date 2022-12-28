//! Collection of archive file formats

#[cfg(feature = "cpio")]
mod cpio;

#[cfg(feature = "tar")]
mod tar;
#[cfg(feature = "tar")]
pub use self::tar::Tar;
