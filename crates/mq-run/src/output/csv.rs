//! CSV output rendering for the `--output-format csv` CLI option.
//!
//! Converts [`mq_lang::RuntimeValue`]s into CSV using the `csv` crate. A top-level
//! `Array` is expanded so each element becomes its own row (mirroring `table.rs`).
//! Arrays of dicts produce a header row from the union of all keys; arrays of arrays
//! treat the first row as the header; anything else becomes a single "value" column.

use miette::miette;
use mq_lang::RuntimeValue;
#[cfg(test)]
use mq_lang::Shared;
use std::collections::BTreeSet;

fn cell_value(value: &RuntimeValue) -> String {
    match value {
        RuntimeValue::Dict(_) | RuntimeValue::Array(_) => {
            serde_json::to_string(&value.clone().to_json_value()).unwrap_or_default()
        }
        RuntimeValue::Markdown(node, _) => node.to_string(),
        _ => value.to_string(),
    }
}

/// Converts a list of [`RuntimeValue`]s into a CSV string.
pub(crate) fn runtime_values_to_csv(runtime_values: &[RuntimeValue]) -> miette::Result<String> {
    let non_none: Vec<&RuntimeValue> = runtime_values.iter().filter(|v| !v.is_none()).collect();

    // unwrap a single top-level Array, matching table.rs's behavior
    let expanded: Option<Vec<&RuntimeValue>> = if let [RuntimeValue::Array(items)] = non_none.as_slice() {
        Some(items.iter().collect())
    } else {
        None
    };
    let candidates: &[&RuntimeValue] = expanded.as_deref().unwrap_or(&non_none);

    let mut writer = csv::WriterBuilder::new().from_writer(vec![]);

    let all_dicts = !candidates.is_empty() && candidates.iter().all(|v| matches!(*v, RuntimeValue::Dict(_)));
    let all_arrays = !candidates.is_empty() && candidates.iter().all(|v| matches!(*v, RuntimeValue::Array(_)));

    let write_err = |e: csv::Error| miette!("Failed to write CSV record: {}", e);

    if all_dicts {
        let mut header_set: BTreeSet<String> = BTreeSet::new();
        for val in candidates.iter() {
            if let RuntimeValue::Dict(map) = *val {
                for key in map.keys() {
                    header_set.insert(key.to_string());
                }
            }
        }
        let headers: Vec<String> = header_set.into_iter().collect();
        writer.write_record(&headers).map_err(write_err)?;

        for val in candidates.iter() {
            if let RuntimeValue::Dict(map) = *val {
                let row: Vec<String> = headers
                    .iter()
                    .map(|h| {
                        map.get(&mq_lang::Ident::new(h.as_str()))
                            .map(cell_value)
                            .unwrap_or_default()
                    })
                    .collect();
                writer.write_record(&row).map_err(write_err)?;
            }
        }
    } else if all_arrays {
        for val in candidates.iter() {
            if let RuntimeValue::Array(items) = *val {
                let row: Vec<String> = items.iter().map(cell_value).collect();
                writer.write_record(&row).map_err(write_err)?;
            }
        }
    } else {
        writer.write_record(["value"]).map_err(write_err)?;
        for val in candidates.iter() {
            writer.write_record([cell_value(val)]).map_err(write_err)?;
        }
    }

    let bytes = writer.into_inner().map_err(|e| miette!("Failed to write CSV: {}", e))?;
    String::from_utf8(bytes).map_err(|e| miette!("Failed to convert CSV output to UTF-8: {}", e))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_array_of_dicts() {
        let mut m1 = std::collections::BTreeMap::new();
        m1.insert(mq_lang::Ident::new("name"), RuntimeValue::String("Alice".to_string()));
        m1.insert(mq_lang::Ident::new("age"), RuntimeValue::String("30".to_string()));
        let mut m2 = std::collections::BTreeMap::new();
        m2.insert(mq_lang::Ident::new("name"), RuntimeValue::String("Bob".to_string()));
        m2.insert(mq_lang::Ident::new("age"), RuntimeValue::String("25".to_string()));
        let values = vec![RuntimeValue::Array(Shared::new(vec![
            RuntimeValue::Dict(Shared::new(m1)),
            RuntimeValue::Dict(Shared::new(m2)),
        ]))];
        let result = runtime_values_to_csv(&values).unwrap();
        assert_eq!(result, "age,name\n30,Alice\n25,Bob\n");
    }

    #[test]
    fn test_single_dict() {
        let mut map = std::collections::BTreeMap::new();
        map.insert(mq_lang::Ident::new("a"), RuntimeValue::String("1".to_string()));
        let values = vec![RuntimeValue::Dict(Shared::new(map))];
        let result = runtime_values_to_csv(&values).unwrap();
        assert_eq!(result, "a\n1\n");
    }

    #[test]
    fn test_needs_quoting() {
        let mut map = std::collections::BTreeMap::new();
        map.insert(mq_lang::Ident::new("a"), RuntimeValue::String("has,comma".to_string()));
        let values = vec![RuntimeValue::Dict(Shared::new(map))];
        let result = runtime_values_to_csv(&values).unwrap();
        assert_eq!(result, "a\n\"has,comma\"\n");
    }

    #[test]
    fn test_scalars_single_column() {
        let values = vec![
            RuntimeValue::String("x".to_string()),
            RuntimeValue::String("y".to_string()),
        ];
        let result = runtime_values_to_csv(&values).unwrap();
        assert_eq!(result, "value\nx\ny\n");
    }

    #[test]
    fn test_missing_key_is_empty() {
        let mut m1 = std::collections::BTreeMap::new();
        m1.insert(mq_lang::Ident::new("a"), RuntimeValue::String("1".to_string()));
        m1.insert(mq_lang::Ident::new("b"), RuntimeValue::String("2".to_string()));
        let mut m2 = std::collections::BTreeMap::new();
        m2.insert(mq_lang::Ident::new("a"), RuntimeValue::String("3".to_string()));
        let values = vec![RuntimeValue::Array(Shared::new(vec![
            RuntimeValue::Dict(Shared::new(m1)),
            RuntimeValue::Dict(Shared::new(m2)),
        ]))];
        let result = runtime_values_to_csv(&values).unwrap();
        assert_eq!(result, "a,b\n1,2\n3,\n");
    }

    #[test]
    fn test_array_of_arrays() {
        let values = vec![RuntimeValue::Array(Shared::new(vec![
            RuntimeValue::Array(Shared::new(vec![
                RuntimeValue::String("h1".to_string()),
                RuntimeValue::String("h2".to_string()),
            ])),
            RuntimeValue::Array(Shared::new(vec![
                RuntimeValue::String("v1".to_string()),
                RuntimeValue::String("v2".to_string()),
            ])),
        ]))];
        let result = runtime_values_to_csv(&values).unwrap();
        assert_eq!(result, "h1,h2\nv1,v2\n");
    }

    #[test]
    fn test_none_filtered_out() {
        let values = vec![RuntimeValue::None, RuntimeValue::String("visible".to_string())];
        let result = runtime_values_to_csv(&values).unwrap();
        assert_eq!(result, "value\nvisible\n");
    }
}
