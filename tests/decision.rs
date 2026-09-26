use anyhow::Result;
use qa_intent::{
    compiler::{Compiler, QaItem},
    model::Answer,
    taxonomy::Taxonomy,
};
use serde_json::{json, Value};

fn setup() -> Result<(Taxonomy, QaItem)> {
    let taxonomy: Taxonomy = serde_json::from_str(include_str!("../examples/taxonomy.json"))?;
    let item = serde_json::from_value(json!({
        "id": "ticket-01", "question": "这个工单怎么处理", "qid": "intent_type",
        "expected": "lookup", "tags": ["lookup"]
    }))?;
    Ok((taxonomy, item))
}

#[test]
fn decisions_validate_labels_and_do_not_mix_confidence_metrics() -> Result<()> {
    let (taxonomy, item) = setup()?;
    let compiled = Compiler::new(&taxonomy).compile(&item)?;
    assert!(compiled.envelope.state.get("expected").is_none());
    assert!(compiled.envelope.state.get("tags").is_none());
    let decide = |value: Value| compiled.decide(&serde_json::from_value::<Answer>(value)?);
    let good = decide(
        json!({"type":"choice","choice":"lookup","answer_confidence":0.9,"confidence":0.3}),
    )?;
    assert!(good.accepted);
    assert_eq!(good.action.as_deref(), Some("route_to_solver"));
    let entropy_only = decide(json!({"type":"choice","choice":"lookup","confidence":0.99}))?;
    assert!(!entropy_only.accepted);
    assert!(entropy_only.action.is_none());
    let low = decide(json!({"type":"choice","choice":"lookup","answer_confidence":0.6}))?;
    assert!(!low.accepted);
    assert!(decide(json!({"type":"choice","choice":"unknown","answer_confidence":0.9})).is_err());
    assert!(decide(json!({"type":"choice","choice":"lookup","answer_confidence":1.5})).is_err());
    assert!(decide(json!({"type":"noul","noul":0.9})).is_err());
    Ok(())
}

#[test]
fn noul_negative_and_uncertain_do_not_trigger_positive_actions() -> Result<()> {
    let (taxonomy, mut item) = setup()?;
    item.qid = "equivalent".into();
    item.expected = Some(Value::Bool(false));
    let compiled = Compiler::new(&taxonomy).compile(&item)?;
    for (p, accepted) in [(0.05, true), (0.65, false), (0.95, true)] {
        let answer: Answer = serde_json::from_value(json!({"type":"noul","noul":p}))?;
        let result = compiled.decide(&answer)?;
        assert_eq!(result.accepted, accepted);
        assert_eq!(result.action.is_some(), p == 0.95);
    }
    Ok(())
}

#[test]
fn dynamic_options_cannot_change_type_or_drop_duplicate_labels() -> Result<()> {
    let (taxonomy, mut item) = setup()?;
    item.options = Some(serde_json::from_value(json!([
        {"key":"lookup","description":"查询"}, {"key":"lookup","description":"重复"}
    ]))?);
    assert!(Compiler::new(&taxonomy).compile(&item).is_err());
    item.qid = "equivalent".into();
    item.options = Some(serde_json::from_value(json!([
        {"key":"yes","description":"是"}, {"key":"no","description":"否"}
    ]))?);
    assert!(Compiler::new(&taxonomy).compile(&item).is_err());
    Ok(())
}

#[test]
fn supervised_export_preserves_labels_outside_request() -> Result<()> {
    let (taxonomy, _) = setup()?;
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("examples/items.json");
    let mut output = Vec::new();
    qa_intent::dataset::export(&taxonomy, &path, &mut output)?;
    let rows: Vec<Value> = String::from_utf8(output)?
        .lines()
        .map(serde_json::from_str)
        .collect::<Result<_, _>>()?;
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0]["target"], true);
    assert_eq!(rows[1]["target"], "common_sense");
    assert!(rows[0]["request"]["state"].get("expected").is_none());
    Ok(())
}
