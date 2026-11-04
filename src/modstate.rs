use serde_json::Value;

#[derive(Clone, Debug, Default)]
#[allow(dead_code)]
pub struct ModState {
    pub json: Value,
    pub in_game: bool,
    pub in_queue: bool,
    pub mm_state: Option<i64>,
    pub game_phase: Option<i64>,
    pub lobby_phase: Option<i64>,
    pub hero: String,
    pub view_hero: String,
    pub account: String,
    pub panel: String,
    pub arranged_match: Option<i64>,
    pub arranged: Option<bool>,
    pub world: String,
    pub lobby_mode: String,
    pub match_id: String,
    pub level: Option<i64>,
    pub kills: Option<i64>,
    pub deaths: Option<i64>,
    pub assists: Option<i64>,
    pub clock: Option<i64>,
    pub local_client: Option<i64>,
    pub view_client: Option<i64>,
}

fn as_string(v: &Value) -> Option<String> {
    match v {
        Value::String(s) => Some(s.clone()),
        Value::Number(n) => Some(n.to_string()),
        Value::Bool(b) => Some(b.to_string()),
        _ => None,
    }
}

fn as_i64(v: &Value) -> Option<i64> {
    match v {
        Value::Number(n) => n.as_i64(),
        Value::String(s) => s.trim().parse().ok(),
        _ => None,
    }
}

fn truthy(v: &Value) -> bool {
    match v {
        Value::Bool(b) => *b,
        Value::Number(n) => n.as_i64().map(|i| i != 0).unwrap_or(false),
        Value::String(s) => matches!(s.trim().to_ascii_lowercase().as_str(), "1" | "true"),
        _ => false,
    }
}

pub fn parse(line: &str) -> Option<ModState> {
    let idx = line.find("[HONRP]")?;
    let json: Value = serde_json::from_str(line[idx + 7..].trim()).ok()?;
    if json.get("boot").is_some() {
        return None;
    }
    let get = |k: &str| json.get(k).cloned().unwrap_or(Value::Null);
    let s = |k: &str| as_string(&get(k)).unwrap_or_default();
    Some(ModState {
        in_game: truthy(&get("in_game")),
        in_queue: truthy(&get("in_queue")),
        mm_state: as_i64(&get("mm_state")),
        game_phase: as_i64(&get("game_phase")),
        lobby_phase: as_i64(&get("lobby_phase")),
        hero: s("hero"),
        view_hero: s("view_hero"),
        account: s("account"),
        panel: s("panel"),
        arranged_match: as_i64(&get("arranged_match")),
        arranged: json.get("arranged").map(truthy),
        world: s("world"),
        lobby_mode: s("lobby_mode"),
        match_id: s("match_id"),
        level: as_i64(&get("level")),
        kills: as_i64(&get("kills")),
        deaths: as_i64(&get("deaths")),
        assists: as_i64(&get("assists")),
        clock: as_i64(&get("clock")),
        local_client: as_i64(&get("local_client")),
        view_client: as_i64(&get("view_client")),
        json,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_string_and_number_fields() {
        let line = r#"[HONRP] {"in_game":"1","in_queue":"false","game_phase":2,"hero":"Hero_Pyromancer","kills":"4","clock":754}"#;
        let m = parse(line).unwrap();
        assert!(m.in_game);
        assert!(!m.in_queue);
        assert_eq!(m.game_phase, Some(2));
        assert_eq!(m.hero, "Hero_Pyromancer");
        assert_eq!(m.kills, Some(4));
        assert_eq!(m.clock, Some(754));
    }

    #[test]
    fn ignores_boot_lines() {
        assert!(parse(r#"[HONRP] {"boot":1,"build":9}"#).is_none());
    }
}
