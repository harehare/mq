//! Parser for gron-style assignment statements (`path = value;`), used by
//! `_gron_parse` to reconstruct a data structure from the flat, greppable
//! output produced by mq's `-F gron` output format.
//!
//! Each line is a path (dot/bracket notation, matching the `gron` tool)
//! followed by `= <json-value>` and an optional trailing `;`. Intermediate
//! containers are auto-vivified from the path itself, so lines can be
//! processed in any order and partial (e.g. `grep`-filtered) input still
//! reconstructs the paths that remain.

#[derive(Debug)]
enum Segment {
    Key(String),
    Index(usize),
}

/// Parses gron assignment statements into a [`serde_json::Value`].
pub(crate) fn parse(input: &str) -> Result<serde_json::Value, String> {
    let mut root = serde_json::Value::Null;

    for (lineno, raw_line) in input.lines().enumerate() {
        let line = raw_line.trim();
        if line.is_empty() {
            continue;
        }

        let (path, value_str) = split_path_value(line).map_err(|e| format!("line {}: {}", lineno + 1, e))?;
        let value_str = value_str.strip_suffix(';').unwrap_or(value_str).trim();
        let value: serde_json::Value = serde_json::from_str(value_str)
            .map_err(|e| format!("line {}: invalid value '{}': {}", lineno + 1, value_str, e))?;
        let segments = parse_path_segments(path).map_err(|e| format!("line {}: {}", lineno + 1, e))?;

        set_path(&mut root, &segments, value);
    }

    Ok(root)
}

/// Splits a line into its path and value parts at the top-level `=`
/// (i.e. one that isn't inside a `[...]` bracket segment of the path).
fn split_path_value(line: &str) -> Result<(&str, &str), String> {
    let bytes = line.as_bytes();
    let mut i = 0;

    while i < bytes.len() {
        match bytes[i] {
            b'[' => {
                i += 1;
                if i < bytes.len() && (bytes[i] == b'"' || bytes[i] == b'\'') {
                    let quote = bytes[i];
                    i += 1;
                    while i < bytes.len() && bytes[i] != quote {
                        i += if bytes[i] == b'\\' { 2 } else { 1 };
                    }
                    i += 1;
                }
                while i < bytes.len() && bytes[i] != b']' {
                    i += 1;
                }
                i += 1;
            }
            b'=' => break,
            _ => i += 1,
        }
    }

    if i >= bytes.len() {
        return Err(format!("expected '=' in '{}'", line));
    }

    Ok((line[..i].trim_end(), line[i + 1..].trim_start()))
}

/// Parses a gron path (e.g. `json.a["b c"][0]`) into a list of key/index
/// segments, skipping the leading root identifier.
fn parse_path_segments(path: &str) -> Result<Vec<Segment>, String> {
    let bytes = path.as_bytes();
    let mut i = 0;

    // Skip the root identifier; its name is not significant.
    while i < bytes.len() && bytes[i] != b'.' && bytes[i] != b'[' {
        i += 1;
    }

    let mut segments = Vec::new();

    while i < bytes.len() {
        match bytes[i] {
            b'.' => {
                i += 1;
                let start = i;
                while i < bytes.len() && bytes[i] != b'.' && bytes[i] != b'[' {
                    i += 1;
                }
                if start == i {
                    return Err(format!("empty key in path '{}'", path));
                }
                segments.push(Segment::Key(path[start..i].to_string()));
            }
            b'[' => {
                i += 1;
                if i < bytes.len() && (bytes[i] == b'"' || bytes[i] == b'\'') {
                    let quote = bytes[i];
                    let str_start = i;
                    i += 1;
                    while i < bytes.len() && bytes[i] != quote {
                        i += if bytes[i] == b'\\' { 2 } else { 1 };
                    }
                    if i >= bytes.len() {
                        return Err(format!("unterminated quoted key in path '{}'", path));
                    }
                    let str_end = i + 1;
                    i += 1;
                    let raw = &path[str_start..str_end];
                    let key = if quote == b'"' {
                        serde_json::from_str::<String>(raw)
                            .map_err(|e| format!("invalid quoted key {} in path '{}': {}", raw, path, e))?
                    } else {
                        raw[1..raw.len() - 1].replace("\\'", "'")
                    };
                    segments.push(Segment::Key(key));
                } else {
                    let start = i;
                    while i < bytes.len() && bytes[i] != b']' {
                        i += 1;
                    }
                    let idx_str = &path[start..i];
                    let idx: usize = idx_str
                        .parse()
                        .map_err(|_| format!("invalid array index '{}' in path '{}'", idx_str, path))?;
                    segments.push(Segment::Index(idx));
                }
                if i >= bytes.len() || bytes[i] != b']' {
                    return Err(format!("expected ']' in path '{}'", path));
                }
                i += 1;
            }
            _ => return Err(format!("unexpected character in path '{}'", path)),
        }
    }

    Ok(segments)
}

/// Writes `value` at `segments` within `root`, auto-vivifying intermediate
/// objects/arrays as needed. A redundant empty-container declaration
/// (`path = {};` / `path = [];`) for a path that already holds richer data
/// is ignored rather than overwriting it, so lines may be applied out of order.
fn set_path(root: &mut serde_json::Value, segments: &[Segment], value: serde_json::Value) {
    if segments.is_empty() {
        let is_empty_container = matches!(&value, serde_json::Value::Object(m) if m.is_empty())
            || matches!(&value, serde_json::Value::Array(a) if a.is_empty());
        if is_empty_container && !root.is_null() {
            return;
        }
        *root = value;
        return;
    }

    match &segments[0] {
        Segment::Key(key) => {
            if !root.is_object() {
                *root = serde_json::Value::Object(serde_json::Map::new());
            }
            let entry = root
                .as_object_mut()
                .unwrap()
                .entry(key.clone())
                .or_insert(serde_json::Value::Null);
            set_path(entry, &segments[1..], value);
        }
        Segment::Index(idx) => {
            if !root.is_array() {
                *root = serde_json::Value::Array(Vec::new());
            }
            let arr = root.as_array_mut().unwrap();
            if arr.len() <= *idx {
                arr.resize(idx + 1, serde_json::Value::Null);
            }
            set_path(&mut arr[*idx], &segments[1..], value);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;
    use serde_json::json;

    #[rstest]
    #[case("json = \"hello\";", json!("hello"))]
    #[case("json = 3;", json!(3))]
    #[case("json = true;", json!(true))]
    #[case("json = null;", serde_json::Value::Null)]
    #[case("json = {};\njson.a = 1;", json!({"a": 1}))]
    #[case("json = [];\njson[0] = \"x\";\njson[1] = \"y\";", json!(["x", "y"]))]
    #[case("json = {};\njson.a = {};\njson.a.b = \"deep\";", json!({"a": {"b": "deep"}}))]
    #[case("json[\"weird key\"] = \"v\";", json!({"weird key": "v"}))]
    #[case("json.arr[2] = \"z\";", json!({"arr": [null, null, "z"]}))]
    // Container decl arriving after its child (e.g. after `sort -r`) must not wipe it.
    #[case("json.a.b = \"deep\";\njson.a = {};", json!({"a": {"b": "deep"}}))]
    // Leaf-only input (as if grep-filtered out of a larger dump) still reconstructs.
    #[case("json.a.b = \"deep\";", json!({"a": {"b": "deep"}}))]
    fn test_parse(#[case] input: &str, #[case] expected: serde_json::Value) {
        assert_eq!(parse(input).unwrap(), expected);
    }

    #[test]
    fn test_parse_ignores_blank_lines() {
        assert_eq!(parse("\njson = 1;\n\n").unwrap(), json!(1));
    }

    #[rstest]
    #[case("json.a")]
    #[case("json.a = ")]
    #[case("json.a = notjson;")]
    #[case("json[abc] = 1;")]
    #[case("json[0 = 1;")]
    fn test_parse_errors(#[case] input: &str) {
        assert!(parse(input).is_err());
    }
}
