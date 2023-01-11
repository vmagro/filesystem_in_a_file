#!/bin/bash
set -ex

SCRIPT_DIR=$( cd -- "$( dirname -- "${BASH_SOURCE[0]}" )" &> /dev/null && pwd )
pushd "$OUT_DIR"

SUDO_ASKPASS=/bin/false sudo true
CAN_MAKE_BTRFS="$?"

# produce the demo filesystem
if [ "$CAN_MAKE_BTRFS" -eq "0" ]; then
    truncate -s 1G image.btrfs
    mkfs.btrfs -f image.btrfs
    mkdir mnt
    sudo mount image.btrfs mnt
    sudo chown -R "$(whoami)" mnt
    btrfs subvolume create mnt/fs
    pushd mnt
else
    rm -rf fs
    mkdir fs
fi
pushd fs
mkdir testdata
echo "Lorem ipsum" > testdata/lorem.txt
setfattr -n user.demo -v "lorem ipsum" testdata/lorem.txt
mkdir testdata/dir
echo "Lorem ipsum dolor sit amet" > testdata/dir/lorem.txt
ln -s ../lorem.txt testdata/dir/symlink

tar --xattrs -cf "$OUT_DIR"/testdata.tar testdata
find testdata | cpio -o -H newc > "$OUT_DIR"/testdata.cpio

popd
if [ "$CAN_MAKE_BTRFS" -eq "0" ]; then
    sudo btrfs property set fs ro true
    sudo btrfs subvolume snapshot fs fs2
    sudo btrfs property set fs2 ro false
    touch fs2/wow
    sudo btrfs property set fs2 ro true
    sudo btrfs send fs -e -f "$OUT_DIR"/testdata.sendstream.1
    sudo btrfs send -p fs fs2 -f "$OUT_DIR"/testdata.sendstream.2
    popd
    sudo umount mnt
    rmdir mnt
    rm image.btrfs
    sudo cat "$OUT_DIR"/testdata.sendstream.1 "$OUT_DIR"/testdata.sendstream.2 > "$OUT_DIR"/testdata.sendstream
    sudo rm "$OUT_DIR"/testdata.sendstream.1 "$OUT_DIR"/testdata.sendstream.2
    sudo chown "$(whoami)" "$OUT_DIR"/testdata.sendstream
else
    cp --reflink=auto "$SCRIPT_DIR"/testdata.sendstream "$OUT_DIR"/testdata.sendstream
fi

popd
