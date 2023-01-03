use bitflags::bitflags;

bitflags! {
    /// Some attributes are expected to be dynamic or unsupported for certain
    /// formats, so we need a way to exclude them from comparisons instead of
    /// always demanding full equality.
    pub struct Fields: u32 {
        /// Complete set of paths in two filesystems must be equal
        const PATH      = 0b1;
        /// The entry type (regular file, device node, etc).
        const TYPE      = 0b10;
        /// File data as it would appear to read(2).
        const DATA      = 0b100;
        /// Raw extents as the data is physically laid out on disk.
        const EXTENTS   = 0b1000;
        /// All file times (ctime, atime, mtime)
        const TIME      = 0b10000;
        /// All xattr names and values
        const XATTR     = 0b100000;
        /// File mode (st_mode)
        const MODE      = 0b1000000;
        /// Owning user/group
        const OWNER     = 0b10000000;
        /// Metadata accessible with stat(2)
        const STAT      = Self::TYPE.bits | Self::TIME.bits | Self::MODE.bits | Self::OWNER.bits;
    }
}

impl Fields {
    /// Fields on a hard filesystem entry, in other words everything but the
    /// path.
    pub fn all_entry_fields() -> Self {
        Self::all() - Self::PATH
    }
}

pub trait ApproxEq<O = Self>: PartialEq<O> {
    /// Return all the flags for fields that are equal. This will be ANDed
    /// together with other comparisons, so should return [Fields::all] with any
    /// not-equal bits unset, rather than only returning relevant and equal
    /// bits. Implementations must be careful to not exit early with any flags
    /// set for fields that might still be equal, since a caller might ignore
    /// certain fields which would cause an incomplete comparison.
    fn cmp(&self, other: &O) -> Fields {
        match self == other {
            true => Fields::all(),
            false => Fields::empty(),
        }
    }

    fn approx_eq(&self, other: &O, fields: Fields) -> bool {
        let cmp = self.cmp(other);
        cmp.contains(fields)
    }
}

#[cfg(test)]
macro_rules! assert_approx_eq {
    ($left:expr, $right:expr, $fields:expr) => {
        let cmp = crate::cmp::ApproxEq::cmp(&$left, &$right);
        let cmp = cmp | $fields.complement();
        assert!(
            cmp.is_all(),
            "{:?} != {:?}. These attributes are not equal: {:?}",
            $left,
            $right,
            cmp.complement(),
        );
    };
    ($left:expr, $right:expr, $fields:expr, $($arg:tt)+) => {
        assert!(
            crate::cmp::ApproxEq::approx_eq(&$left, &$right, $fields),
            $arg
        );
    };
}

#[cfg(test)]
pub(crate) use assert_approx_eq;
