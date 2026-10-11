//! Builds the vendored, unmodified libexpat sources plus the oracle shims.
//!
//! The configuration header lives in `shim/expat_config.h` (a documented
//! build shim, see docs/DECISIONS.md D-008); nothing under `vendor/` is
//! modified.

fn main() {
    let lib = "vendor/expat/lib";
    for f in [
        "xmlparse.c",
        "xmlrole.c",
        "xmltok.c",
        "xcs.c",
        "random_dev_urandom.c",
    ] {
        println!("cargo:rerun-if-changed={lib}/{f}");
    }
    for f in [
        "shim/expat_config.h",
        "shim/oracle.h",
        "shim/oracle_trace.c",
        "shim/oracle_internals.c",
    ] {
        println!("cargo:rerun-if-changed={f}");
    }
    let mut build = cc::Build::new();
    build
        .include("shim")
        .include(lib)
        .file(format!("{lib}/xmlparse.c"))
        .file(format!("{lib}/xmlrole.c"))
        .file(format!("{lib}/xmltok.c"))
        .file(format!("{lib}/xcs.c"))
        .file(format!("{lib}/random_dev_urandom.c"))
        .file("shim/oracle_trace.c")
        .file("shim/oracle_internals.c")
        .define("XML_STATIC", None)
        .warnings(false);
    // Under `cargo fuzz` (which sets `--cfg fuzzing`), instrument the C code
    // too, so libFuzzer is guided by coverage of the oracle and not only of
    // the Rust side, and AddressSanitizer covers the C heap.
    println!("cargo:rerun-if-env-changed=CARGO_CFG_FUZZING");
    if std::env::var_os("CARGO_CFG_FUZZING").is_some() {
        build
            .compiler("clang")
            .flag("-fsanitize=fuzzer-no-link")
            .flag("-fsanitize=address");
    }
    build.compile("expat_oracle");
}
