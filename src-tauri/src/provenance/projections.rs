use super::{canonical::canonical_digest, records::RecordReference, CplResult};
use serde_json::{json, Value};

pub fn deterministic_projection(
    project_id: &str,
    chain_head_sha256: &str,
    mut records: Vec<RecordReference>,
) -> CplResult<Value> {
    records.sort_by(|left, right| left.record_id.as_bytes().cmp(right.record_id.as_bytes()));
    let content = json!({
        "project_id": project_id,
        "chain_head_sha256": chain_head_sha256,
        "records": records,
    });
    Ok(json!({
        "content_sha256": canonical_digest(&content)?,
        "content": content,
    }))
}
