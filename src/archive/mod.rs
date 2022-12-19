//! Collection of archive file formats

#[cfg(feature = "cpio")]
mod cpio;
#[cfg(feature = "cpio")]
pub use self::cpio::Cpio;

#[cfg(feature = "tar")]
mod tar;
#[cfg(feature = "tar")]
pub use self::tar::Tar;
