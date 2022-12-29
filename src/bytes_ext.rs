use bytes::Bytes;

pub(crate) trait BytesExt {
    /// See [Bytes::slice_ref]. This provides a way to safely check ahead of
    /// time if the subslice is actually in the bounds of the [Bytes]
    fn is_subslice(&self, subslice: &[u8]) -> bool;

    /// See [Bytes::slice_ref]. This is a version that either slices into the
    /// existing [Bytes], or copies it into a new [Bytes].
    fn subslice_or_copy(&self, subslice: &[u8]) -> Bytes;
}

impl BytesExt for Bytes {
    fn is_subslice(&self, subslice: &[u8]) -> bool {
        let bytes_p = self.as_ptr() as usize;
        let bytes_len = self.len();

        let sub_p = subslice.as_ptr() as usize;
        let sub_len = subslice.len();

        (sub_p >= bytes_p) && (sub_p + sub_len <= bytes_p + bytes_len)
    }

    fn subslice_or_copy(&self, subslice: &[u8]) -> Bytes {
        if self.is_subslice(subslice) {
            self.slice_ref(subslice)
        } else {
            Bytes::copy_from_slice(subslice)
        }
    }
}
