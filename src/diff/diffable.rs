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

#[derive(Debug, Clone)]
pub struct DiffSection<'a> {
    pub(super) title: &'static str,
    pub(super) contents: Cow<'a, str>,
}

pub trait Diffable<'a, const N: usize>: Sized + Debug + ApproxEq {
    /// Return a string representation of this object. Not directly used for
    /// comparison ([ApproxEq] will be used for that), but will be used to
    /// display the diff to the user.
    fn to_diffable_sections(&'a self) -> [DiffSection<'a>; N];
}

impl<'a, T: Diffable<'a, N>, const N: usize> Diffable<'a, N> for &'_ T {
    fn to_diffable_sections(&'a self) -> [DiffSection<'a>; N] {
        (**self).to_diffable_sections()
    }
}

impl<'a> Diffable<'a, 3> for Entry {
    fn to_diffable_sections(&'a self) -> [DiffSection<'a>; 3] {
        [
            DiffSection {
                title: "Type",
                contents: Cow::Borrowed(match self {
                    Self::File(_) => "File",
                    Self::Directory(_) => "Directory",
                    Self::Special(_) => "Special",
                    Self::Symlink(_) => "Symlink",
                }),
            },
            DiffSection {
                title: "Metadata",
                contents: Cow::Owned(format!("{:#?}", self.metadata())),
            },
            DiffSection {
                title: "Contents",
                contents: match self {
                    Self::File(x) => x.diffable_contents(),
                    Self::Directory(_) => Cow::Borrowed(""),
                    Self::Special(x) => Cow::Owned(x.diffable_contents()),
                    Self::Symlink(x) => Cow::Borrowed(x.diffable_contents()),
                },
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
    use nix::unistd::Gid;
    use nix::unistd::Uid;
    use similar_asserts::assert_eq;

    use super::*;
    use crate::diff::Diff;
    use crate::entry::Metadata;
    use crate::File;

    #[test]
    fn text_file_entry_diff_is_useful() {
        let diff = Diff::Changed {
            left: Entry::from(
                File::builder()
                    .contents("Lorem ipsum\ndolor sit amet\n")
                    .metadata(
                        Metadata::builder()
                            .mode(Mode::from_bits_truncate(0o644))
                            .uid(Uid::current())
                            .gid(Gid::current())
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
                            .uid(Uid::current())
                            .gid(Gid::current())
                            .xattr("user.demo", "dolor")
                            .build(),
                    )
                    .build(),
            ),
        };
        assert_eq!(
            r#"Type
Metadata
@@ -1,5 +1,5 @@
 Metadata {
-    mode: S_IRUSR | S_IWUSR | S_IRGRP | S_IROTH,
+    mode: S_IRUSR | S_IRGRP | S_IROTH,
     uid: Uid(
         1000,
     ),
@@ -7,7 +7,7 @@
         1000,
     ),
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
            console::strip_ansi_codes(&diff.to_string())
        )
    }
}
