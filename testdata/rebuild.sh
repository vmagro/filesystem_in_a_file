#!/bin/bash
set -ex

SCRIPT_DIR=$( cd -- "$( dirname -- "${BASH_SOURCE[0]}" )" &> /dev/null && pwd )
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
ln -s ../lorem.txt testdata/dir/symlink

tar --xattrs -cf "$OUT_DIR"/testdata.tar testdata
find testdata | cpio -o -H newc > "$OUT_DIR"/testdata.cpio

# TODO: build the sendstream in this script instead of just copying it
cp --reflink=auto "$SCRIPT_DIR/demo.sendstream" "$OUT_DIR"/testdata.sendstream

popd
popd
