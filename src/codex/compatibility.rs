use serde_json::{Map, Value};
pub const ALLOWED: &[&str] = &[
    "model",
    "instructions",
    "input",
    "tools",
    "tool_choice",
    "parallel_tool_calls",
    "store",
    "stream",
    "include",
    "reasoning",
    "service_tier",
    "prompt_cache_key",
    "text",
    "previous_response_id",
];
pub fn transform(root: &Value) -> Option<Value> {
    let obj = root.as_object()?;
    let mut out = Map::new();
    for k in ALLOWED {
        if let Some(v) = obj.get(*k) {
            out.insert((*k).into(), v.clone());
        }
    }
    out.insert("stream".into(), Value::Bool(true));
    out.insert("store".into(), Value::Bool(false));
    if !out.get("instructions").is_some_and(Value::is_string) {
        out.insert("instructions".into(), Value::String(String::new()));
    }
    Some(Value::Object(out))
}
#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn golden_filter() {
        let v=transform(&json!({"model":"gpt-5","input":"hi","temperature":1,"max_output_tokens":4,"store":true,"stream":false})).unwrap();
        assert_eq!(v["stream"], true);
        assert_eq!(v["store"], false);
        assert_eq!(v["instructions"], "");
        assert!(v.get("temperature").is_none());
        assert!(v.get("max_output_tokens").is_none());
    }
}
