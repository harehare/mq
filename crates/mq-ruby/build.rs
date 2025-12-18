use std::process::Command;

fn main() {
    // Get Ruby library directory
    let libdir = Command::new("ruby")
        .args(["-e", "puts RbConfig::CONFIG['libdir']"])
        .output()
        .expect("Failed to execute ruby command")
        .stdout;
    let libdir = String::from_utf8(libdir).unwrap().trim().to_string();

    // Get Ruby library name
    let libruby = Command::new("ruby")
        .args(["-e", "puts RbConfig::CONFIG['LIBRUBY']"])
        .output()
        .expect("Failed to execute ruby command")
        .stdout;
    let libruby = String::from_utf8(libruby).unwrap().trim().to_string();

    // Remove 'lib' prefix and '.dylib'/'.so' suffix to get the library name
    let lib_name = libruby
        .strip_prefix("lib")
        .unwrap_or(&libruby)
        .strip_suffix(".dylib")
        .or_else(|| libruby.strip_suffix(".so"))
        .unwrap_or(&libruby);

    // Output linker directives
    println!("cargo:rustc-link-search=native={}", libdir);
    println!("cargo:rustc-link-lib=dylib={}", lib_name);

    // Rerun if Ruby changes
    println!("cargo:rerun-if-env-changed=RUBY");
    println!("cargo:rerun-if-env-changed=RUBY_ROOT");
}
