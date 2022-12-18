use std::io::Read;

use super::File;

/// Read implementation for File structs
pub struct Reader<'r, 'f> {
    file: &'r mut File<'f>,
    pos: usize,
}

impl<'f> File<'f> {
    pub fn reader<'r>(&'r mut self) -> Reader<'r, 'f> {
        Reader { file: self, pos: 0 }
    }
}

impl<'r, 'f> Read for Reader<'r, 'f> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        if self.pos >= self.file.len() {
            return Ok(0);
        }
        match self.file.extent_for_byte(self.pos) {
            Some((extent_start, ext)) => {
                let remaining_in_extent = extent_start + ext.len() - self.pos;
                let read_len = std::cmp::min(buf.len(), remaining_in_extent);
                let extent_offset = extent_start - self.pos;
                buf[..read_len]
                    .copy_from_slice(&ext.data()[extent_offset..extent_offset + read_len]);
                self.pos += read_len;
                return Ok(read_len);
            }
            // this is impossible due to the length check above
            None => unreachable!("cannot read past end of file"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::tests::test_file;
    use super::*;

    #[test]
    fn read_all() {
        let mut f = test_file();
        let mut buf = Vec::new();
        f.reader().read_to_end(&mut buf).expect("infallible");
        // file::tests::to_bytes already ensures that to_bytes is correct
        assert_eq!(buf, f.to_bytes());
    }
}
