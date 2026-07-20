use super::records::CplRecord;

const PREDICATES: &[&str] = &[
    "derived_from",
    "generated_by",
    "modified_by_human",
    "selected_by_human",
    "arranged_by_human",
    "included_in_deposit",
];

const EVALUATION_STATUSES: &[&str] = &["exact", "degraded", "refused", "stale", "unverified"];

pub fn validate_record(record: &CplRecord) -> Result<(), String> {
    match record.record_type.as_str() {
        "provenance-assertion" => {
            let predicate = record
                .payload
                .get("predicate")
                .and_then(|value| value.as_str())
                .ok_or("A provenance assertion requires a predicate.")?;
            if !PREDICATES.contains(&predicate) {
                return Err(format!(
                    "Unknown composition assertion predicate '{predicate}'."
                ));
            }
            if !record
                .payload
                .get("dependencies")
                .is_some_and(|value| value.is_array())
            {
                return Err("A provenance assertion requires explicit dependencies.".to_owned());
            }
        }
        "assertion-evaluation" => {
            let status = record
                .payload
                .get("status")
                .and_then(|value| value.as_str())
                .ok_or("An assertion evaluation requires a status.")?;
            if !EVALUATION_STATUSES.contains(&status) {
                return Err(format!("Unknown assertion evaluation status '{status}'."));
            }
            if record
                .payload
                .get("evaluated_against")
                .and_then(|value| value.get("chain_head"))
                .and_then(|value| value.as_str())
                .is_none()
            {
                return Err("An assertion evaluation must bind an explicit chain head.".to_owned());
            }
        }
        _ => {}
    }
    Ok(())
}
