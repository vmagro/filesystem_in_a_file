#!/bin/bash
set -ex

pushd "$OUT_DIR"

# produce the demo filesystem
rm -rf fs
mkdir fs
pushd fs
mkdir testdata
echo "Lorem ipsum" > testdata/lorem.txt
setfattr -n user.demo -v "lorem ipsum" testdata/lorem.txt
mkdir testdata/dir
echo "Lorem ipsum dolor sit amet" > testdata/dir/lorem.txt

tar --xattrs -cf "$OUT_DIR"/testdata.tar testdata
find testdata | cpio -o -H newc > "$OUT_DIR"/testdata.cpio

popd
popd
