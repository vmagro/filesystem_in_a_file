use std::path::Path;

use crate::BytesPath;
use crate::Entry;
use crate::Filesystem;
use crate::InodeKey;

impl<'f> IntoIterator for &'f Filesystem {
    type Item = (&'f Path, &'f Entry);
    type IntoIter = Iter<'f>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

impl Filesystem {
    pub fn iter(&self) -> Iter {
        Iter {
            iter: self.paths.iter(),
            fs: self,
        }
    }
}

pub struct Iter<'f> {
    fs: &'f Filesystem,
    iter: std::collections::btree_map::Iter<'f, BytesPath, InodeKey>,
}

impl<'f> Iterator for Iter<'f> {
    type Item = (&'f Path, &'f Entry);

    fn next(&mut self) -> Option<Self::Item> {
        self.iter.next().map(|(path, inode)| {
            (
                path.as_ref(),
                self.fs.inodes.get(*inode).expect("must exist"),
            )
        })
    }
}
