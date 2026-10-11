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
    build.compile("expat_oracle");
}
