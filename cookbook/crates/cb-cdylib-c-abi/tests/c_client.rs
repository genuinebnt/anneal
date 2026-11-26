//! Build the library with cargo, compile the C client with `cc`, link it
//! (statically and dynamically), run it and assert stdout + exit code.
use std::path::{Path, PathBuf};
use std::process::Command;

const EXPECTED: &str = "\
value=22 history_sum=47
null add rc=-1 msg=counter is NULL
Hello, C!
small rc=-3 need=15
big rc=0 text=rust-owned-data
panic rc=-5 msg=panic: deliberate panic
point size=12 x=3 y=4
";

fn build_lib() -> PathBuf {
    // Own target dir: avoids deadlocking on the outer cargo's lock and
    // guarantees the cdylib/staticlib artifacts exist (tests only need the rlib).
    let target = std::env::temp_dir().join("cb_cdylib_c_abi_target");
    let st = Command::new(env!("CARGO"))
        .args([
            "build",
            "-q",
            "--release",
            "--manifest-path",
            concat!(env!("CARGO_MANIFEST_DIR"), "/Cargo.toml"),
        ])
        .env("CARGO_TARGET_DIR", &target)
        .status()
        .unwrap();
    assert!(st.success());
    target.join("release")
}

fn run(exe: &Path) {
    let out = Command::new(exe).output().unwrap();
    assert_eq!(String::from_utf8_lossy(&out.stdout), EXPECTED);
    assert_eq!(out.status.code(), Some(0));
}

fn compile(out: &Path, link_args: &[&str]) {
    let dir = env!("CARGO_MANIFEST_DIR");
    let st = Command::new("cc")
        .args(["-std=c11", "-Wall", "-Wextra", "-Werror", "-I"])
        .arg(format!("{dir}/include"))
        .arg(format!("{dir}/tests/c/client.c"))
        .args(link_args)
        .arg("-o")
        .arg(out)
        .status()
        .expect("cc not found");
    assert!(st.success(), "C compile/link failed");
}

#[test]
fn c_program_links_static_and_dynamic() {
    let lib = build_lib();
    let out = lib.join("c_client_out");
    std::fs::create_dir_all(&out).unwrap();

    // Static: name the .a explicitly; no runtime lookup needed.
    let static_exe = out.join("client_static");
    compile(
        &static_exe,
        &[lib.join("libcb_cdylib_c_abi.a").to_str().unwrap()],
    );
    run(&static_exe);

    // Dynamic: -L/-l picks the .dylib/.so (shared preferred over .a); rpath lets
    // the loader find it next to the build output.
    // Hide the .a from the linker's search so -l cannot choose it.
    let dyn_dir = out.join("dyn");
    std::fs::create_dir_all(&dyn_dir).unwrap();
    let ext = if cfg!(target_os = "macos") {
        "dylib"
    } else {
        "so"
    };
    let so = lib.join(format!("libcb_cdylib_c_abi.{ext}"));
    std::fs::copy(&so, dyn_dir.join(so.file_name().unwrap())).unwrap();
    let dyn_exe = out.join("client_dyn");
    let d = dyn_dir.to_str().unwrap();
    compile(
        &dyn_exe,
        &[
            &format!("-L{d}"),
            "-lcb_cdylib_c_abi",
            &format!("-Wl,-rpath,{d}"),
        ],
    );
    run(&dyn_exe);
}
