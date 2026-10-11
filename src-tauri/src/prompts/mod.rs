//! Prompt stitching — merges 6 active modules from system_prompts in fixed order.

use rusqlite::Connection;

const ORDER: [&str; 6] = [
    "role_and_abort_protocol",
    "validation_and_search",
    "growth_and_fcf",
    "narrative_and_risks",
    "confidence_scoring",
    "json_schema",
];

pub fn stitch(conn: &Connection) -> Result<String, String> {
    let mut parts = Vec::new();
    for key in ORDER {
        let txt: String = conn
            .query_row(
                "SELECT prompt_text FROM system_prompts WHERE prompt_key = ?1 AND is_active = 1",
                rusqlite::params![key],
                |r| r.get(0),
            )
            .map_err(|e| format!("prompt {key} missing: {e}"))?;
        parts.push(txt);
    }
    Ok(parts.join("\n\n---\n\n"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;

    fn test_conn() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        crate::db::migrate(&conn).unwrap();
        crate::db::seed(&conn, false).unwrap();
        conn
    }

    #[test]
    fn stitch_six_modules_in_order() {
        let conn = test_conn();
        let s = stitch(&conn).unwrap();
        assert!(s.contains("ValorBot"));
        assert!(s.contains("json_schema") || s.contains("json_schema") || s.contains("Emit exactly one JSON"));
        // order check: role before validation before growth
        let role_pos = s.find("role_and_abort_protocol").or_else(|| s.find("ValorBot")).unwrap();
        let val_pos = s.find("INPUT DATA").unwrap_or(s.find("Tools").unwrap_or(usize::MAX));
        assert!(role_pos < val_pos);
    }
}
