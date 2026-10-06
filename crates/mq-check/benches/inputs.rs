//! Benchmark inputs shared by `benchmark.rs` and `examples/phase_profile.rs`.

/// Named inputs: a tiny program, real-world code of builtin.mq size, and synthetic code with
/// `N` functions sharing parameter names (the quadratic case in name resolution and calls).
pub const INPUTS: &[&str] = &["small", "builtin", "synthetic_200", "synthetic_1000", "synthetic_4000"];

pub fn input(name: &str) -> String {
    match name {
        "small" => "def f(x): x + 1; | f(2)".to_string(),
        "builtin" => include_str!("../../mq-lang/builtin.mq").to_string(),
        "synthetic_200" => synthetic(200),
        "synthetic_1000" => synthetic(1000),
        "synthetic_4000" => synthetic(4000),
        _ => panic!("unknown input {name}"),
    }
}

/// Whether the input defines the builtins itself and must be checked without them.
pub fn is_builtin_source(name: &str) -> bool {
    name == "builtin"
}

/// `count` three-line functions over shared parameter names, each called once.
fn synthetic(count: usize) -> String {
    let mut code = String::new();
    for i in 0..count {
        code.push_str(&format!(
            "def f{i}(x, y):\n  let z = x + y\n  | if (z > {i}): upcase(to_string(z)) else: downcase(\"{i}\");\n"
        ));
    }
    for i in 0..count {
        code.push_str(&format!("| f{i}({i}, 2)\n"));
    }
    code
}
