use std::borrow::Cow;
use std::fmt::Debug;
use std::fmt::Write;
use std::hash::Hasher;
use std::os::unix::prelude::OsStrExt;

use twox_hash::XxHash64;

use crate::cmp::ApproxEq;
use crate::entry::Entry;
use crate::entry::Metadata;
use crate::entry::Special;
use crate::entry::Symlink;
use crate::file::File;

pub(crate) trait Diffable: Sized + Debug + ApproxEq {
    /// Return a string representation of this object. Not directly used for
    /// comparison ([ApproxEq] will be used for that), but will be used to
    /// display the diff to the user.
    fn to_diffable_string<'a>(&'a self) -> Cow<'a, str> {
        Cow::Owned(format!("{self:#?}"))
    }
}

impl<T: Diffable> Diffable for &'_ T {
    fn to_diffable_string<'a>(&'a self) -> Cow<'a, str> {
        (**self).to_diffable_string()
    }
}

impl Diffable for Metadata {}

impl Diffable for Entry {
    fn to_diffable_string(&self) -> Cow<'_, str> {
        let mut s = String::new();
        s.push_str(match self {
            Self::File(_) => "File\n",
            Self::Directory(_) => "Directory\n",
            Self::Special(_) => "Special\n",
            Self::Symlink(_) => "Symlink\n",
        });
        s.push_str(&self.metadata().to_diffable_string());
        s.push_str("\n==========\n");
        match self {
            Self::File(x) => s.push_str(&x.to_diffable_string()),
            Self::Directory(_) => (),
            Self::Special(x) => s.push_str(&x.to_diffable_string()),
            Self::Symlink(x) => s.push_str(&x.to_diffable_string()),
        };
        Cow::Owned(s)
    }
}

impl Diffable for File {
    fn to_diffable_string(&self) -> Cow<'_, str> {
        let contents = self.to_bytes();
        match std::str::from_utf8(&contents) {
            Ok(contents) => contents.to_owned().into(),
            Err(_) => {
                let mut hasher = XxHash64::with_seed(0);
                hasher.write(&contents);
                format!("binary data: xxHash = {}", hasher.finish()).into()
            }
        }
    }
}

impl Diffable for Special {
    fn to_diffable_string<'a>(&'a self) -> Cow<'a, str> {
        let mut s = String::new();
        write!(s, "{:?}", self.file_type()).expect("infallible");
        if let Some(rdev) = self.rdev() {
            write!(s, " rdev({rdev})").expect("infallbile");
        }
        Cow::Owned(s)
    }
}

impl Diffable for Symlink {
    fn to_diffable_string<'a>(&'a self) -> Cow<'a, str> {
        Cow::Borrowed(
            std::str::from_utf8(self.target().as_os_str().as_bytes())
                .expect("our paths are always valid utf8"),
        )
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
    use crate::File;

    #[test]
    fn text_file_entry_diff_is_useful() {
        let diff = Diff::Changed {
            left: Entry::from(
                File::builder()
                    .contents("Lorem ipsum\ndolor sit amet")
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
                    .contents("Lorem ipsum\nconsectetur adipiscing elit\ndolor sit")
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
            r#"Differences (-left|+right):
 File
 Metadata {
-    mode: S_IRUSR | S_IWUSR | S_IRGRP | S_IROTH,
+    mode: S_IRUSR | S_IRGRP | S_IROTH,
     uid: Uid(
         1000,
     ),
     gid: Gid(
         1000,
     ),
     xattrs: {
-        b"user.demo": b"lorem ipsum",
+        b"user.demo": b"dolor",
     },
     created: SystemTime {
         tv_sec: 0,
         tv_nsec: 0,
     },
     accessed: SystemTime {
         tv_sec: 0,
         tv_nsec: 0,
     },
     modified: SystemTime {
         tv_sec: 0,
         tv_nsec: 0,
     },
 }
 ==========
 Lorem ipsum
-dolor sit amet
+consectetur adipiscing elit
+dolor sit
"#,
            console::strip_ansi_codes(&diff.to_string())
        )
    }
}
