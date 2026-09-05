use http::{header, HeaderMap};
pub fn upstream_response_headers(source: &HeaderMap) -> HeaderMap {
    let mut out = HeaderMap::new();
    for name in [header::CONTENT_TYPE, header::CACHE_CONTROL] {
        if let Some(v) = source.get(&name) {
            out.insert(name, v.clone());
        }
    }
    for name in ["x-request-id", "openai-request-id"] {
        if let Some(v) = source.get(name) {
            out.insert(name, v.clone());
        }
    }
    out
}
