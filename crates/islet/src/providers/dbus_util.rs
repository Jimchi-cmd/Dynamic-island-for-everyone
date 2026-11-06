//! Small helpers for reading loosely typed D-Bus values (players and
//! battery daemons differ in the exact integer and string types they use).

use std::collections::HashMap;
use zbus::zvariant::Value;

/// Unwrap nested variants (`v` inside `a{sv}`).
pub fn inner<'a, 'b>(v: &'b Value<'a>) -> &'b Value<'a> {
    match v {
        Value::Value(b) => inner(b),
        other => other,
    }
}

pub fn str_of(v: &Value) -> Option<String> {
    match inner(v) {
        Value::Str(s) => Some(s.to_string()),
        Value::ObjectPath(p) => Some(p.to_string()),
        _ => None,
    }
}

pub fn i64_of(v: &Value) -> Option<i64> {
    Some(match inner(v) {
        Value::I64(x) => *x,
        Value::U64(x) => *x as i64,
        Value::I32(x) => *x as i64,
        Value::U32(x) => *x as i64,
        Value::I16(x) => *x as i64,
        Value::U16(x) => *x as i64,
        Value::U8(x) => *x as i64,
        Value::F64(x) => *x as i64,
        _ => return None,
    })
}

pub fn f64_of(v: &Value) -> Option<f64> {
    match inner(v) {
        Value::F64(x) => Some(*x),
        other => i64_of(other).map(|i| i as f64),
    }
}

pub fn bool_of(v: &Value) -> Option<bool> {
    match inner(v) {
        Value::Bool(b) => Some(*b),
        _ => None,
    }
}

/// A string or an array of strings, as a list.
pub fn str_list_of(v: &Value) -> Vec<String> {
    match inner(v) {
        Value::Array(a) => a.inner().iter().filter_map(str_of).collect(),
        other => str_of(other).into_iter().collect(),
    }
}

/// An `a{sv}` dictionary as owned key → value.
pub fn dict_of<'a>(v: &Value<'a>) -> HashMap<String, Value<'a>> {
    match inner(v) {
        Value::Dict(d) => d.iter().filter_map(|(k, v)| Some((str_of(k)?, v.try_clone().ok()?))).collect(),
        _ => HashMap::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_loosely_typed_values() {
        assert_eq!(i64_of(&Value::from(5u64)), Some(5));
        assert_eq!(i64_of(&Value::Value(Box::new(Value::from(7i32)))), Some(7));
        assert_eq!(f64_of(&Value::from(3u32)), Some(3.0));
        assert_eq!(str_of(&Value::from("x")), Some("x".into()));
        assert_eq!(bool_of(&Value::from(true)), Some(true));
        assert_eq!(str_list_of(&Value::from(vec!["a", "b"])), ["a", "b"]);
        assert_eq!(str_list_of(&Value::from("solo")), ["solo"]);
    }
}
