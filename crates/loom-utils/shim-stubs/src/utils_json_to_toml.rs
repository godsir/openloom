// Stub for codex-utils-json-to-toml types.

use serde_json::Value as JsonValue;

/// Stub: converts JSON value to a TOML value.
/// Returns an empty TOML table for non-object values.
pub fn json_to_toml(v: JsonValue) -> toml_edit::Value {
    match v {
        JsonValue::String(s) => toml_edit::Value::from(s),
        JsonValue::Number(n) => {
            if let Some(i) = n.as_i64() {
                toml_edit::Value::from(i)
            } else if let Some(f) = n.as_f64() {
                toml_edit::Value::from(f)
            } else {
                toml_edit::Value::from(0i64)
            }
        }
        JsonValue::Bool(b) => toml_edit::Value::from(b),
        JsonValue::Array(_arr) => {
            // Simplified: return empty array
            toml_edit::Value::Array(toml_edit::Array::new())
        }
        JsonValue::Object(_obj) => {
            // Simplified: return empty table
            toml_edit::Value::InlineTable(toml_edit::InlineTable::new())
        }
        JsonValue::Null => toml_edit::Value::from(""),
    }
}
