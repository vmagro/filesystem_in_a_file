use std::borrow::Cow;
use std::io::Seek;
use std::io::Write;

use super::Extent;
use super::File;

/// [Write] implementation for File structs
pub struct Writer<'r, 'f> {
    file: &'r mut File<'f>,
    pos: usize,
}

impl<'f> File<'f> {
    /// Open a [Writer] at the end of the file. Use [Seek] to move around if you
    /// want to write somewhere in the middle
    pub fn writer<'r>(&'r mut self) -> Writer<'r, 'f> {
        Writer {
            pos: self.len(),
            file: self,
        }
    }
}

impl<'r, 'f> Writer<'r, 'f> {
    /// Write some bytes into the [File] without making a copy of the underlying
    /// data like the [std::io::Write] implementation is forced to do.
    pub fn write<B>(&mut self, buf: B)
    where
        B: Into<Cow<'f, [u8]>>,
    {
        let extent = Extent::Owned(buf.into());
        let ext_len = extent.len();
        // TODO: handle overlapping writes after implementing seek
        self.file.extents.insert(self.pos, extent);
        self.pos += ext_len;
    }
}

impl<'r, 'f> Write for Writer<'r, 'f> {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.write(buf.to_vec());
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn appending_writes() {
        let mut f = File::new();
        let mut w = f.writer();
        w.write_all(b"Lorem ipsum").expect("infallible");
        w.write_all(b" dolor sit amet").expect("infallible");
        assert_eq!(f.to_bytes(), b"Lorem ipsum dolor sit amet");
        assert_eq!(f.extents.len(), 2);
    }
}
