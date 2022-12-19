use std::borrow::Cow;
use std::io::Cursor;

use tar::Archive;
use tar::EntryType;

use crate::File;
use crate::Filesystem;

impl<'f> Filesystem<'f, 'f> {
    /// Create an in-memory view of a Filesystem from the contents of a tarball.
    pub fn from_tar(contents: &'f [u8]) -> std::io::Result<Self> {
        let mut fs = Self::new();
        for entry in Archive::new(Cursor::new(contents)).entries_with_seek()? {
            let entry = entry?;
            let file_offset = entry.raw_file_position() as usize;
            let path = Cow::Owned(entry.path()?.to_path_buf());
            if entry.header().entry_type() == EntryType::Regular {
                fs.files.insert(
                    path,
                    File::from(&contents[file_offset..file_offset + entry.size() as usize]).into(),
                );
            }
        }
        Ok(fs)
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::path::Path;

    use super::*;

    #[test]
    fn tar() {
        let fs = Filesystem::from_tar(include_bytes!("testdata/testdata.tar"))
            .expect("failed to parse tar");
        assert_eq!(
            fs,
            Filesystem {
                files: BTreeMap::from([
                    (
                        Path::new("./lorem.txt").into(),
                        File::from(b"Lorem ipsum\n").into()
                    ),
                    (
                        Path::new("./dir/lorem.txt").into(),
                        File::from(b"Lorem ipsum dolor sit amet\n").into()
                    ),
                ]),
            }
        );
    }
}
