//! Every digest custodian takes (a spec's, a receipt's, an option word's place in a key) is stable_digest's SHA-256
//! over one canonical byte form, declared here once:
//!
//! ```text
//! custodian.canonical.v1\n<what>\n<the value as JSON, object keys sorted, no whitespace>
//! ```
//!
//! `what` names the thing digested (`spec`, `receipt`, `search`), so two different things with the same JSON never
//! share a digest. Keys are sorted here, not by serde_json's map, so the form does not depend on a feature another
//! crate turns on.

use serde::Serialize;
use serde_json::Value;
use stable_digest::StableDigest;

/// The canonical form's first line.
pub const CANONICAL: &str = "custodian.canonical.v1";

/// The canonical bytes of `value`, as `what`.
pub fn canonical<T: Serialize + ?Sized>(what: &str, value: &T) -> Vec<u8> {
    let v = serde_json::to_value(value).unwrap_or(Value::Null);
    let mut out = format!("{CANONICAL}\n{what}\n").into_bytes();
    write_sorted(&v, &mut out);
    out
}

fn write_sorted(v: &Value, out: &mut Vec<u8>) {
    match v {
        Value::Object(map) => {
            let mut keys: Vec<&String> = map.keys().collect();
            keys.sort();
            out.push(b'{');
            for (i, k) in keys.iter().enumerate() {
                if i > 0 {
                    out.push(b',');
                }
                out.extend(serde_json::to_vec(k).unwrap_or_default());
                out.push(b':');
                write_sorted(&map[k.as_str()], out);
            }
            out.push(b'}');
        }
        Value::Array(items) => {
            out.push(b'[');
            for (i, item) in items.iter().enumerate() {
                if i > 0 {
                    out.push(b',');
                }
                write_sorted(item, out);
            }
            out.push(b']');
        }
        other => out.extend(serde_json::to_vec(other).unwrap_or_default()),
    }
}

/// The digest of `value` as `what`: 64 lowercase hex characters.
pub fn of<T: Serialize + ?Sized>(what: &str, value: &T) -> String {
    StableDigest::sha256(canonical(what, value)).as_str().to_owned()
}

/// The digest of raw bytes (a file).
pub fn of_bytes(bytes: impl AsRef<[u8]>) -> String {
    StableDigest::sha256(bytes).as_str().to_owned()
}

/// The first `n` characters, for a line a person reads.
pub fn short(d: &str, n: usize) -> &str {
    &d[..d.len().min(n)]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_form_sorts_keys_and_names_what_it_is() {
        let a = serde_json::json!({"b": 1, "a": [{"y": 2, "x": 1}]});
        let b = serde_json::json!({"a": [{"x": 1, "y": 2}], "b": 1});
        assert_eq!(canonical("spec", &a), canonical("spec", &b));
        assert_eq!(String::from_utf8(canonical("spec", &a)).unwrap(), "custodian.canonical.v1\nspec\n{\"a\":[{\"x\":1,\"y\":2}],\"b\":1}");
        assert_ne!(of("spec", &a), of("receipt", &a));
        assert_eq!(of("spec", &a).len(), 64);
    }
}
