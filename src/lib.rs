use std::borrow::Cow;
use std::collections::BTreeMap;
use std::path::Path;

pub mod file;
#[cfg(feature = "tar")]
pub mod tar;

use file::File;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Filesystem<'p, 'f> {
    files: BTreeMap<Cow<'p, Path>, Entry<'f>>,
}

impl<'p, 'f> Filesystem<'p, 'f> {
    fn new() -> Self {
        Self {
            files: BTreeMap::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Entry<'f> {
    File(File<'f>),
}

impl<'f> From<File<'f>> for Entry<'f> {
    fn from(f: File<'f>) -> Self {
        Self::File(f)
    }
}
