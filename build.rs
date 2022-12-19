use std::process::Command;

fn main() {
    println!("cargo:rerun-if-changed=testdata/rebuild.sh");
    let out = Command::new("testdata/rebuild.sh").spawn().unwrap().wait().unwrap();
    assert!(out.success());
}
