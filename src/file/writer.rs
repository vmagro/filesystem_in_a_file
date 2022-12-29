use std::io::Error;
use std::io::ErrorKind;
use std::io::Result;
use std::io::Seek;
use std::io::SeekFrom;

use super::Extent;
use super::File;

/// [Write] implementation for [File]s
pub struct Writer<'r> {
    file: &'r mut File,
    pos: u64,
}

impl File {
    /// Open a [Writer] at the end of the file. Use [Seek] to move around if you
    /// want to write somewhere in the middle
    pub fn writer<'r>(&'r mut self) -> Writer<'r> {
        Writer {
            pos: self.len(),
            file: self,
        }
    }
}

impl<'r> Writer<'r> {
    /// Write some bytes into the [File] without making a copy of the underlying
    /// data like the [std::io::Write] implementation is forced to do.
    pub fn write<E>(&mut self, extent: E)
    where
        E: Into<Extent>,
    {
        let extent = extent.into();
        let ext_len = extent.len();
        let write_start = self.pos;
        let write_end = write_start + ext_len;
        if let Some((existing_start, existing_ext)) = self.file.extent_for_byte_mut(write_end) {
            let right = existing_ext.split_at((write_end - existing_start) as usize);
            self.file.extents.insert(write_end, right);
        }
        if let Some((existing_start, existing_ext)) = self.file.extent_for_byte_mut(self.pos) {
            // TODO: handle overlapping writes after implementing seek
            // shrink this extent to end where the overlap is
            let split_idx = write_start - existing_start;
            let right_split_idx = write_end - split_idx;
            let mut right = existing_ext.split_at(split_idx as usize);
            if right_split_idx < right.len() {
                right.split_at(right_split_idx as usize);
                let right_start = write_end;
                self.file.extents.insert(right_start, right);
            }
        }
        self.file.extents.insert(self.pos, extent);
        self.pos += ext_len;
    }
}

impl<'r> Seek for Writer<'r> {
    fn seek(&mut self, seek: SeekFrom) -> Result<u64> {
        let (base_pos, offset) = match seek {
            SeekFrom::Start(n) => {
                self.pos = n;
                return Ok(n);
            }
            SeekFrom::End(n) => (self.file.len(), n),
            SeekFrom::Current(n) => (self.pos, n),
        };
        match base_pos.checked_add_signed(offset) {
            Some(n) => {
                self.pos = n;
                Ok(self.pos)
            }
            None => Err(Error::new(
                ErrorKind::InvalidInput,
                "invalid seek to a negative or overflowing position",
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;

    #[test]
    fn appending_writes() {
        let mut f = File::new_empty();
        let mut w = f.writer();
        w.write("Lorem ipsum");
        w.write(" dolor sit amet");
        assert_eq!(f.to_bytes(), b"Lorem ipsum dolor sit amet");
        assert_eq!(f.extents.len(), 2);
    }

    #[test]
    fn overwrite() {
        let mut f = File::new_empty();
        let mut w = f.writer();
        w.write("Lorem lorem");
        w.seek(SeekFrom::Start("Lorem ".len() as u64))
            .expect("infallible");
        w.write("ipsum dolor sit amet");
        assert_eq!(f.to_bytes(), b"Lorem ipsum dolor sit amet");
        assert_eq!(f.extents.len(), 2);
        assert_eq!(
            &f.extents,
            &BTreeMap::from([
                (0, "Lorem ".into()),
                ("Lorem ".len() as u64, "ipsum dolor sit amet".into()),
            ]),
        );
    }

    #[test]
    fn internal_overwrite() {
        let mut f = File::new_empty();
        let mut w = f.writer();
        w.write("Lorem lorem dolor sit amet");
        w.seek(SeekFrom::Start("Lorem ".len() as u64))
            .expect("infallible");
        w.write("ipsum");
        assert_eq!(
            std::str::from_utf8(&f.to_bytes()).expect("valid"),
            "Lorem ipsum dolor sit amet",
            "{f:?}",
        );
        assert_eq!(f.extents.len(), 3);
        assert_eq!(
            &f.extents,
            &BTreeMap::from([
                (0, "Lorem ".into()),
                ("Lorem ".len() as u64, "ipsum".into()),
                ("Lorem ipsum".len() as u64, " dolor sit amet".into()),
            ]),
        );
    }
}
