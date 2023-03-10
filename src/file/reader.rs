use std::io::Read;

use super::File;

/// [Read] implementation for [File]
pub struct Reader<'r> {
    file: &'r File,
    pos: u64,
}

impl File {
    pub fn reader(&self) -> Reader<'_> {
        Reader { file: self, pos: 0 }
    }
}

impl<'r> Read for Reader<'r> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        if self.pos >= self.file.len() {
            return Ok(0);
        }
        match self.file.extent_for_byte(self.pos) {
            Some((extent_start, ext)) => {
                let remaining_in_extent = extent_start + ext.len() - self.pos;
                let read_len = std::cmp::min(buf.len(), remaining_in_extent as usize);
                let extent_offset = self.pos - extent_start;
                buf[..read_len].copy_from_slice(
                    &ext.data()[extent_offset as usize..extent_offset as usize + read_len],
                );
                self.pos += read_len as u64;
                Ok(read_len)
            }
            // this is impossible due to the length check above
            None => {
                unreachable!(
                    "cannot read past end of file (pos = {}, file = {:?}",
                    self.pos, self.file,
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::io::Seek;
    use std::io::SeekFrom;

    use super::super::tests::test_file;
    use super::*;

    #[test]
    fn read_all() {
        let f = test_file();
        let mut buf = Vec::new();
        f.reader().read_to_end(&mut buf).expect("infallible");
        // file::tests::to_bytes already ensures that to_bytes is correct
        assert_eq!(buf, f.to_bytes().as_ref());
    }

    #[test]
    fn overlapping_writes() {
        let mut f = File::new_empty();
        let mut w = f.writer();
        w.write("Lorem lorem");
        w.seek(SeekFrom::Start("Lorem ".len() as u64))
            .expect("infallible");
        w.write("ipsum dolor sit amet");
        let mut buf = Vec::new();
        f.reader().read_to_end(&mut buf).expect("infallible");
        assert_eq!(
            std::str::from_utf8(&buf).expect("is utf8"),
            "Lorem ipsum dolor sit amet",
            "{f:?}",
        );
        assert_eq!(f.extents.len(), 2);
    }
}
