//! Generates the C header.

use std::env;
use std::path::Path;

fn main() {
    let crate_dir = env::var("CARGO_MANIFEST_DIR").unwrap();

    let bindings = cbindgen::Builder::new()
        .with_language(cbindgen::Language::C)
        .with_crate(&crate_dir)
        .with_include_guard("MQ_H")
        .with_cpp_compat(true)
        .rename_item("MqContext", "mq_context_t")
        .rename_item("MqResult", "mq_result_t")
        .rename_item("MqStatus", "mq_status_t")
        .rename_item("MqError", "mq_error_t")
        .rename_item("MqLocation", "mq_location_t")
        .rename_item("MqValues", "mq_values_t")
        .rename_item("MqType", "mq_type_t")
        .rename_item("MqHtmlOptions", "mq_html_options_t")
        .rename_item("MqPosition", "mq_position_t")
        .generate()
        .unwrap();
    bindings.write_to_file(Path::new(&crate_dir).join("mq.h"));
}
