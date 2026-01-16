use serde::Serialize;
use serde_json::{Map, Value};

use crate::error::{PostUrbitError, Result};

pub fn canonical_json_from<T: Serialize>(value: &T) -> Result<String> {
    let value = serde_json::to_value(value)
        .map_err(|_| PostUrbitError::InvalidInput("serialize to json"))?;
    canonical_json_value(&value)
}

pub fn canonical_json_value(value: &Value) -> Result<String> {
    let normalized = normalize_value(value)?;
    serde_json::to_string(&normalized)
        .map_err(|_| PostUrbitError::InvalidInput("serialize canonical json"))
}

fn normalize_value(value: &Value) -> Result<Value> {
    match value {
        Value::Object(map) => {
            let mut entries: Vec<_> = map.iter().collect();
            entries.sort_by(|a, b| a.0.cmp(b.0));
            let mut new_map = Map::new();
            for (key, val) in entries {
                new_map.insert(key.clone(), normalize_value(val)?);
            }
            Ok(Value::Object(new_map))
        }
        Value::Array(values) => {
            let mut out = Vec::with_capacity(values.len());
            for val in values {
                out.push(normalize_value(val)?);
            }
            Ok(Value::Array(out))
        }
        _ => Ok(value.clone()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn canonical_orders_object_keys() {
        let input = json!({"b": 1, "a": 2});
        let out = canonical_json_value(&input).unwrap();
        assert_eq!(out, r#"{"a":2,"b":1}"#);
    }
}
