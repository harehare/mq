use std::env;
use std::path::Path;

fn main() {
    let crate_dir = env::var("CARGO_MANIFEST_DIR").unwrap();

    let bindings = cbindgen::Builder::new()
        .with_language(cbindgen::Language::C)
        .with_crate(&crate_dir)
        .with_include_guard("MQ_H")
        .rename_item("MqContext", "mq_context_t")
        .rename_item("MqResult", "mq_result_t")
        .generate()
        .unwrap();
    bindings.write_to_file(Path::new(&crate_dir).join("mq.h"));
}
