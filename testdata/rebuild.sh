#!/bin/bash
set -ex

pushd fs

setfattr -n user.demo -v "lorem ipsum" testdata/lorem.txt
tar --xattrs -cf ../testdata.tar testdata
find testdata | cpio -o -H newc > ../testdata.cpio

popd
