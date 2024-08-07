use std::{env, ffi::OsStr, path::PathBuf};

use libbpf_cargo::SkeletonBuilder;

const BPF_SRC: &str = "src/bpf/tracexec_system.bpf.c";

fn main() {
  #[cfg(feature = "ebpf")]
  {
    let manifest_dir =
      PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR not set"));
    let skel_out = manifest_dir
      .clone()
      .join("src")
      .join("bpf")
      .join("tracexec_system.skel.rs");
    let arch = env::var("CARGO_CFG_TARGET_ARCH").expect("CARGO_CFG_TARGET_ARCH not set");
    let arch_define = OsStr::new(match arch.as_str() {
      "x86_64" => "__x86_64__",
      "riscv64" => "__riscv64__",
      "aarch64" => "__aarch64__",
      _ => panic!("Arch {arch} is not supported for now"),
    });

    SkeletonBuilder::new()
      .source(BPF_SRC)
      .clang_args([
        // vmlinux.h
        OsStr::new("-I"),
        manifest_dir.join("include").as_os_str(),
        OsStr::new("-D"),
        arch_define,
      ])
      .build_and_generate(&skel_out)
      .unwrap();
    println!("cargo:rerun-if-changed={BPF_SRC}");
  }
}
