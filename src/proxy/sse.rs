use crate::error::{AppError, Result};
use serde_json::Value;
#[derive(Default)]
pub struct Parser {
    buffer: Vec<u8>,
}
#[derive(Debug)]
pub struct Event {
    pub name: Option<String>,
    pub data: String,
}
impl Event {
    pub fn is_terminal_error(&self) -> bool {
        if matches!(self.name.as_deref(), Some("error" | "response.failed")) {
            return true;
        }
        serde_json::from_str::<Value>(&self.data)
            .ok()
            .and_then(|v| v.get("type").and_then(Value::as_str).map(str::to_owned))
            .is_some_and(|kind| matches!(kind.as_str(), "error" | "response.failed"))
    }
}
impl Parser {
    pub fn push(&mut self, chunk: &[u8]) -> Result<Vec<Event>> {
        self.buffer.extend_from_slice(chunk);
        let mut out = Vec::new();
        loop {
            let Some((at, len)) = separator(&self.buffer) else {
                break;
            };
            let frame = self.buffer.drain(..at).collect::<Vec<_>>();
            self.buffer.drain(..len);
            let text = std::str::from_utf8(&frame)
                .map_err(|_| AppError::Upstream("invalid UTF-8 in upstream SSE".into()))?;
            if let Some(e) = parse_frame(text) {
                out.push(e)
            }
        }
        Ok(out)
    }
    pub fn finish(&self) -> Result<()> {
        if self.buffer.iter().all(|b| b.is_ascii_whitespace()) {
            Ok(())
        } else {
            Err(AppError::Upstream("truncated upstream SSE record".into()))
        }
    }
}
fn separator(b: &[u8]) -> Option<(usize, usize)> {
    b.windows(4)
        .position(|w| w == b"\r\n\r\n")
        .map(|p| (p, 4))
        .or_else(|| b.windows(2).position(|w| w == b"\n\n").map(|p| (p, 2)))
}
fn parse_frame(s: &str) -> Option<Event> {
    let mut name = None;
    let mut data = Vec::new();
    for line in s.lines() {
        let line = line.trim_end_matches('\r');
        if let Some(v) = line.strip_prefix("event:") {
            name = Some(v.trim().to_owned())
        } else if let Some(v) = line.strip_prefix("data:") {
            data.push(v.strip_prefix(' ').unwrap_or(v))
        }
    }
    if data.is_empty() {
        None
    } else {
        Some(Event {
            name,
            data: data.join("\n"),
        })
    }
}
#[derive(Default)]
pub struct Aggregator {
    response: Option<Value>,
    output: Vec<Value>,
    completed: bool,
}
impl Aggregator {
    pub fn new() -> Self {
        Self {
            response: None,
            output: Vec::new(),
            completed: false,
        }
    }
    pub fn accept(&mut self, e: Event) -> Result<()> {
        if e.data == "[DONE]" {
            return Ok(());
        }
        let v: Value = serde_json::from_str(&e.data)
            .map_err(|_| AppError::Upstream("invalid JSON in upstream SSE".into()))?;
        let kind = e
            .name
            .as_deref()
            .or_else(|| v.get("type").and_then(Value::as_str))
            .unwrap_or("");
        match kind {
            "response.created" => self.response = Some(v.get("response").cloned().unwrap_or(v)),
            "response.output_item.done" => {
                let item = v
                    .get("item")
                    .cloned()
                    .ok_or_else(|| AppError::Upstream("output-item event omitted item".into()))?;
                if let Some(id) = item.get("id") {
                    if let Some(old) = self.output.iter_mut().find(|x| x.get("id") == Some(id)) {
                        *old = item
                    } else {
                        self.output.push(item)
                    }
                } else {
                    self.output.push(item)
                }
            }
            "response.completed" => {
                let done = v.get("response").cloned().unwrap_or(v);
                merge(&mut self.response, done);
                self.completed = true
            }
            "error" | "response.failed" => {
                return Err(AppError::Upstream(
                    "upstream SSE reported a terminal error".into(),
                ))
            }
            _ => {}
        }
        Ok(())
    }
    pub fn finish(mut self) -> Result<Value> {
        if !self.completed {
            return Err(AppError::Upstream(
                "upstream SSE ended before response.completed".into(),
            ));
        }
        let mut v = self.response.take().ok_or_else(|| {
            AppError::Upstream("upstream SSE contained no response object".into())
        })?;
        v.as_object_mut()
            .ok_or_else(|| AppError::Upstream("completed response was not an object".into()))?
            .insert("output".into(), Value::Array(self.output));
        Ok(v)
    }
}
fn merge(base: &mut Option<Value>, update: Value) {
    match (base.as_mut(), update.as_object()) {
        (Some(Value::Object(a)), Some(b)) => {
            for (k, v) in b {
                if k != "output" {
                    a.insert(k.clone(), v.clone());
                }
            }
        }
        _ => *base = Some(update),
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn fragmented() {
        let mut p = Parser::default();
        assert!(p.push(b"event: response.com").unwrap().is_empty());
        let e = p
            .push(
                b"pleted\ndata: {\"type\":\"response.completed\",\"response\":{\"id\":\"r\"}}\n\n",
            )
            .unwrap();
        assert_eq!(e.len(), 1);
        let mut a = Aggregator::new();
        a.accept(e.into_iter().next().unwrap()).unwrap();
        assert_eq!(a.finish().unwrap()["id"], "r");
    }
    #[test]
    fn malformed() {
        let mut p = Parser::default();
        assert!(p
            .push(&[b'd', b'a', b't', b'a', b':', 0xff, b'\n', b'\n'])
            .is_err());
    }
}
