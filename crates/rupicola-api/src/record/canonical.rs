use super::RecordError;
use serde_json::Value;

/// The canonical text of a JSON value: object keys sorted by their bytes, no spaces, and no
/// number that is not a whole one.
///
/// # Errors
///
/// [`RecordError::NotWhole`] naming the first number that has a fraction or an exponent.
pub fn canonical_json(value: &Value) -> Result<String, RecordError> {
    let mut out = String::new();
    write_canonical(value, "$", &mut out)?;
    Ok(out)
}

fn write_canonical(value: &Value, at: &str, out: &mut String) -> Result<(), RecordError> {
    match value {
        Value::Null => out.push_str("null"),
        Value::Bool(flag) => out.push_str(if *flag { "true" } else { "false" }),
        Value::Number(number) => {
            if let Some(whole) = number.as_i64() {
                out.push_str(&whole.to_string());
            } else if let Some(whole) = number.as_u64() {
                out.push_str(&whole.to_string());
            } else {
                return Err(RecordError::NotWhole(at.to_owned()));
            }
        }
        Value::String(text) => out.push_str(
            &serde_json::to_string(text).map_err(|e| RecordError::invalid(e.to_string()))?,
        ),
        Value::Array(items) => {
            out.push('[');
            for (index, item) in items.iter().enumerate() {
                if index > 0 {
                    out.push(',');
                }
                write_canonical(item, &format!("{at}[{index}]"), out)?;
            }
            out.push(']');
        }
        Value::Object(map) => {
            let mut entries: Vec<(&String, &Value)> = map.iter().collect();
            entries.sort_by(|a, b| a.0.as_bytes().cmp(b.0.as_bytes()));
            out.push('{');
            for (index, (key, item)) in entries.into_iter().enumerate() {
                if index > 0 {
                    out.push(',');
                }
                out.push_str(
                    &serde_json::to_string(key).map_err(|e| RecordError::invalid(e.to_string()))?,
                );
                out.push(':');
                write_canonical(item, &format!("{at}.{key}"), out)?;
            }
            out.push('}');
        }
    }
    Ok(())
}
