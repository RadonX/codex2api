use codex2api::proxy::sse::{Aggregator, Parser};
#[test]
fn aggregates_records_split_at_every_boundary() {
    let raw=b"event: response.created\ndata: {\"response\":{\"id\":\"r1\",\"status\":\"in_progress\"}}\n\nevent: response.output_item.done\ndata: {\"item\":{\"id\":\"m1\",\"type\":\"message\"}}\n\nevent: response.completed\ndata: {\"response\":{\"id\":\"r1\",\"status\":\"completed\"}}\n\n";
    let mut p = Parser::default();
    let mut a = Aggregator::new();
    for b in raw {
        for e in p.push(&[*b]).unwrap() {
            a.accept(e).unwrap()
        }
    }
    p.finish().unwrap();
    let v = a.finish().unwrap();
    assert_eq!(v["status"], "completed");
    assert_eq!(v["output"][0]["id"], "m1");
}
