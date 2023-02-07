[![Workflow Status](https://github.com/vmagro/filesystem_in_a_file/actions/workflows/main.yml/badge.svg)](https://github.com/vmagro/filesystem_in_a_file/actions)
[![docs.rs](https://img.shields.io/docsrs/filesystem_in_a_file)](https://docs.rs/filesystem_in_a_file)
![Maintenance](https://img.shields.io/badge/maintenance-experimental-blue.svg)

# filesystem_in_a_file

A complete view of a filesystem provided by various archive formats. Currently
this crate supports BTRFS Sendstreams, tarballs, and cpio archives.

The intended use case is to use this in-memory representation to enable
full-filesystem comparisons during integration tests for image packaging tools.

License: MIT
