use std::borrow::Cow;
use std::fmt::Debug;
use std::fmt::Write;
use std::hash::Hasher;
use std::os::unix::prelude::OsStrExt;

use twox_hash::XxHash64;

use crate::cmp::ApproxEq;
use crate::entry::Entry;
use crate::entry::Special;
use crate::entry::Symlink;
use crate::file::File;

pub trait Diffable<'a, const N: usize>: Sized + Debug + ApproxEq {
    const SECTIONS: [&'static str; N];
    /// Return a string representation of this object. Not directly used for
    /// comparison ([ApproxEq] will be used for that), but will be used to
    /// display the diff to the user.
    fn to_diffable_sections(&'a self) -> [Cow<'a, str>; N];
}

impl<'a, T: Diffable<'a, N>, const N: usize> Diffable<'a, N> for &'_ T {
    const SECTIONS: [&'static str; N] = <T as Diffable<'a, N>>::SECTIONS;

    fn to_diffable_sections(&'a self) -> [Cow<'a, str>; N] {
        (**self).to_diffable_sections()
    }
}

impl<'a> Diffable<'a, 3> for Entry {
    const SECTIONS: [&'static str; 3] = ["Type", "Metadata", "Contents"];

    fn to_diffable_sections(&'a self) -> [Cow<'a, str>; 3] {
        [
            Cow::Borrowed(match self {
                Self::File(_) => "File",
                Self::Directory(_) => "Directory",
                Self::Special(_) => "Special",
                Self::Symlink(_) => "Symlink",
            }),
            Cow::Owned(format!("{:#?}", self.metadata())),
            match self {
                Self::File(x) => x.diffable_contents(),
                Self::Directory(_) => Cow::Borrowed(""),
                Self::Special(x) => Cow::Owned(x.diffable_contents()),
                Self::Symlink(x) => Cow::Borrowed(x.diffable_contents()),
            },
        ]
    }
}

impl File {
    fn diffable_contents(&self) -> Cow<'_, str> {
        let contents = self.to_bytes();
        match self.to_bytes() {
            Cow::Borrowed(b) => match std::str::from_utf8(b) {
                Ok(contents) => Cow::Borrowed(contents),
                Err(_) => {
                    let mut hasher = XxHash64::with_seed(0);
                    hasher.write(&contents);
                    Cow::Owned(format!("binary data: xxHash = {}", hasher.finish()))
                }
            },
            Cow::Owned(v) => match String::from_utf8(v) {
                Ok(contents) => Cow::Owned(contents),
                Err(_) => {
                    let mut hasher = XxHash64::with_seed(0);
                    hasher.write(&contents);
                    Cow::Owned(format!("binary data: xxHash = {}", hasher.finish()))
                }
            },
        }
    }
}

impl Special {
    fn diffable_contents(&self) -> String {
        let mut s = String::new();
        write!(s, "{:?}", self.file_type()).expect("infallible");
        if let Some(rdev) = self.rdev() {
            write!(s, " rdev({rdev})").expect("infallbile");
        }
        s
    }
}

impl Symlink {
    fn diffable_contents(&self) -> &str {
        std::str::from_utf8(self.target().as_os_str().as_bytes())
            .expect("our paths are always valid utf8")
    }
}

#[cfg(test)]
mod tests {
    use nix::sys::stat::Mode;
    use similar_asserts::assert_eq;

    use super::*;
    use crate::diff::Diff;
    use crate::entry::Metadata;
    use crate::File;
    use crate::Gid;
    use crate::Uid;

    #[test]
    fn text_file_entry_diff_is_useful() {
        let diff = Diff::Changed {
            left: Entry::from(
                File::builder()
                    .contents("Lorem ipsum\ndolor sit amet\n")
                    .metadata(
                        Metadata::builder()
                            .mode(Mode::from_bits_truncate(0o644))
                            .uid(Uid::from_raw(1000))
                            .gid(Gid::from_raw(1000))
                            .xattr("user.demo", "lorem ipsum")
                            .build(),
                    )
                    .build(),
            ),
            right: Entry::from(
                File::builder()
                    .contents("Lorem ipsum\nconsectetur adipiscing elit\ndolor sit\n")
                    .metadata(
                        Metadata::builder()
                            .mode(Mode::from_bits_truncate(0o444))
                            .uid(Uid::from_raw(1000))
                            .gid(Gid::from_raw(1000))
                            .xattr("user.demo", "dolor")
                            .build(),
                    )
                    .build(),
            ),
        };
        assert_eq!(
            diff.to_string(),
            r#"Metadata
@@ -1,9 +1,9 @@
 Metadata {
-    mode: S_IRUSR | S_IWUSR | S_IRGRP | S_IROTH,
+    mode: S_IRUSR | S_IRGRP | S_IROTH,
     uid: Uid(1000),
     gid: Gid(1000),
     xattrs: {
-        b"user.demo": b"lorem ipsum",
+        b"user.demo": b"dolor",
     },
     created: SystemTime {
         tv_sec: 0,
Contents
@@ -1,2 +1,3 @@
 Lorem ipsum
-dolor sit amet
+consectetur adipiscing elit
+dolor sit
"#,
        )
    }
}
