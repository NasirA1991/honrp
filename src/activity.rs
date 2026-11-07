use crate::config::Config;

#[derive(Default, Clone, Debug)]
pub struct View<'a> {
    pub in_game: bool,
    pub in_queue: bool,
    pub game_phase: Option<i64>,
    pub panel: &'a str,
    pub hero: &'a str,
    pub view_hero: &'a str,
    pub account: &'a str,
    pub arranged_match: Option<i64>,
    pub map: &'a str,
    pub mode: &'a str,
    pub level: Option<i64>,
    pub kills: Option<i64>,
    pub deaths: Option<i64>,
    pub assists: Option<i64>,
    pub local_client: Option<i64>,
    pub view_client: Option<i64>,
    pub match_started: Option<i64>,
    pub session_start: Option<i64>,
}

#[derive(Clone, Debug)]
pub struct Activity {
    pub details: String,
    pub state: String,
    pub start: Option<i64>,
    pub large_image: String,
    pub large_text: String,
}

pub fn in_match(v: &View) -> bool {
    v.in_game || v.game_phase == Some(3)
}

pub fn in_pregame(v: &View) -> bool {
    if in_match(v) {
        return false;
    }
    if matches!(v.game_phase, Some(1) | Some(2)) {
        return true;
    }
    v.panel == "game_lobby" && v.local_client.unwrap_or(-1) >= 0
}

pub fn is_spectating(v: &View) -> bool {
    if !in_match(v) {
        return false;
    }
    match (v.local_client, v.view_client) {
        // A different viewed client is the strong signal. Same client means we
        // are playing, even if GetLocalHeroName() is empty (local/practice).
        (Some(local), Some(view)) => local != view,
        // Without client ids, only use the hero hint for a viewer we know
        // exists (view hero but no local hero).
        (None, Some(_)) => v.hero.is_empty() && !v.view_hero.is_empty(),
        _ => false,
    }
}

fn ranked(v: &View) -> &'static str {
    match v.arranged_match {
        Some(2) | Some(3) => "Ranked",
        Some(4) | Some(5) => "Unranked",
        _ => "",
    }
}

fn title_case(s: &str) -> String {
    s.replace('_', " ")
        .split_whitespace()
        .map(|w| {
            let mut c = w.chars();
            match c.next() {
                Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn map_label(id: &str, short: bool) -> String {
    let key = id.trim().to_ascii_lowercase();
    if key.is_empty() {
        return String::new();
    }
    if short {
        match key.as_str() {
            "caldavar" | "caldavarphoenix" => return "FoC".into(),
            "midwars" => return "Mid Wars".into(),
            "fieldtest" => return "Field Test".into(),
            _ => {}
        }
    }
    match key.as_str() {
        "caldavar" => "Forests of Caldavar".into(),
        "caldavarphoenix" => "Forests of Caldavar (Phoenix)".into(),
        "midwars" => "Mid Wars".into(),
        "fieldtest" => "Field Test".into(),
        "showdown" => "Showdown".into(),
        "blankworld" => "Blank World".into(),
        other => title_case(other),
    }
}

fn mode_label(mode: &str) -> String {
    let key = mode.trim().to_ascii_lowercase();
    let key = key.strip_prefix("mode_").unwrap_or(&key);
    if key.is_empty() {
        return String::new();
    }
    match key {
        "normal" | "allpick" | "all_pick" => "All Pick",
        "banningpick" => "Banning Pick",
        "banningdraft" => "Banning Draft",
        "banningdraftblitz" => "Blitz Banning Draft",
        "blindbanningpick" => "Blind Banning",
        "blindbanningpickblitz" => "Blitz Blind Banning",
        "blindpick" | "blind_pick" => "Blind Pick",
        "randomdraft" | "random_draft" => "Random Draft",
        "singledraft" => "Single Draft",
        "sameherodraft" => "Same Hero Draft",
        "captainsmode" => "Captains Mode",
        "lockpick" => "Lock Pick",
        "shufflepick" => "Shuffle Pick",
        "allrandom" => "All Random",
        "balancedrandom" => "Balanced Random",
        "heroban" => "Hero Ban",
        other => return title_case(other),
    }
    .to_string()
}

pub fn hero_label(name: &str) -> String {
    let mut h = name.trim().to_string();
    if h.to_ascii_lowercase().starts_with("hero_") {
        h = h[5..].to_string();
    }
    if !h.contains(' ') && !h.contains('_') {
        let mut out = String::new();
        let mut prev_lower = false;
        for c in h.chars() {
            if prev_lower && c.is_ascii_uppercase() {
                out.push(' ');
            }
            out.push(c);
            prev_lower = c.is_ascii_lowercase() || c.is_ascii_digit();
        }
        return out;
    }
    h
}

fn phase_label(phase: Option<i64>) -> &'static str {
    match phase {
        Some(1) => "Waiting for players",
        Some(2) => "Hero Select",
        Some(4) => "Post-Match",
        Some(5) => "Match Over",
        _ => "",
    }
}

fn panel_label(panel: &str) -> &'static str {
    match panel {
        "plinko" | "plinko_v2" => "Playing Plinko",
        "store" => "Browsing the Store",
        "shop" => "Browsing the Shop",
        "ladder" => "Viewing the Ladder",
        "profile" => "Viewing a Profile",
        "matchstats" => "Viewing Match Stats",
        "learn" => "In the Learn tab",
        "game_lobby" => "In a game lobby",
        _ => "",
    }
}

fn join(parts: &[&str]) -> String {
    parts
        .iter()
        .filter(|p| !p.is_empty())
        .copied()
        .collect::<Vec<_>>()
        .join(" \u{2022} ")
}

fn clip(s: String) -> String {
    if s.chars().count() > 128 {
        s.chars().take(128).collect()
    } else {
        s
    }
}

pub fn build(v: &View, cfg: &Config) -> Option<Activity> {
    let mut start = None;
    let (details, state);

    if in_match(v) {
        let map = map_label(v.map, cfg.short_maps);
        let mode = mode_label(v.mode);
        let rank = ranked(v);
        start = v.match_started;
        if is_spectating(v) {
            details = join(&["Spectating", &map]);
            state = if v.view_hero.is_empty() {
                "Spectating".to_string()
            } else {
                format!("Watching {}", hero_label(v.view_hero))
            };
        } else {
            details = {
                let d = join(&[rank, &map, &mode]);
                if d.is_empty() {
                    "In a match".to_string()
                } else {
                    d
                }
            };
            let mut bits: Vec<String> = Vec::new();
            let mut hero = v.hero.to_string();
            if hero.is_empty() {
                hero = v.view_hero.to_string();
            }
            if !hero.is_empty() {
                bits.push(hero_label(&hero));
                if let Some(level) = v.level {
                    bits.push(format!("Lv {level}"));
                }
                if cfg.show_kda {
                    if let (Some(k), Some(d), Some(a)) = (v.kills, v.deaths, v.assists) {
                        bits.push(format!("{k}/{d}/{a}"));
                    }
                }
            } else if v.game_phase == Some(4) {
                bits.push("Picking Phase".into());
            } else if !mode.is_empty() {
                bits.push(mode.clone());
            }
            state = if bits.is_empty() {
                "In a match".to_string()
            } else {
                bits.join(" \u{2022} ")
            };
        }
    } else if in_pregame(v) {
        let map = map_label(v.map, cfg.short_maps);
        let mode = mode_label(v.mode);
        details = {
            let d = join(&[ranked(v), &map, &mode]);
            if d.is_empty() {
                "In a game lobby".to_string()
            } else {
                d
            }
        };
        let mut bits: Vec<String> = Vec::new();
        if !v.hero.is_empty() {
            bits.push(hero_label(v.hero));
        }
        let phase = phase_label(v.game_phase);
        if !phase.is_empty() {
            bits.push(phase.to_string());
        }
        state = if bits.is_empty() {
            "In a game lobby".into()
        } else {
            bits.join(" \u{2022} ")
        };
    } else if v.in_queue {
        details = "Searching for a match".into();
        state = "In queue".into();
        start = v.session_start;
    } else {
        details = {
            let p = panel_label(v.panel);
            if p.is_empty() {
                "Main Menu".to_string()
            } else {
                p.to_string()
            }
        };
        state = if !v.account.is_empty() {
            v.account.to_string()
        } else {
            String::new()
        };
        start = v.session_start;
    }

    Some(Activity {
        details: clip(details),
        state: clip(state),
        start: if cfg.show_timer { start } else { None },
        large_image: cfg.large_image.clone(),
        large_text: cfg.large_text.clone(),
    })
}

impl Activity {
    pub fn to_json(&self) -> serde_json::Value {
        let mut obj = serde_json::Map::new();
        obj.insert("details".into(), self.details.clone().into());
        if !self.state.is_empty() {
            obj.insert("state".into(), self.state.clone().into());
        }
        if let Some(start) = self.start {
            obj.insert(
                "timestamps".into(),
                serde_json::json!({ "start": start * 1000 }),
            );
        }
        if !self.large_image.is_empty() {
            obj.insert(
                "assets".into(),
                serde_json::json!({
                    "large_image": self.large_image,
                    "large_text": self.large_text,
                }),
            );
        }
        obj.insert("instance".into(), false.into());
        serde_json::Value::Object(obj)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg() -> Config {
        Config::default()
    }

    fn view<'a>() -> View<'a> {
        View::default()
    }

    #[test]
    fn ranked_match_hero_level_kda() {
        let v = View {
            in_game: true,
            hero: "Hero_Pyromancer",
            level: Some(12),
            kills: Some(4),
            deaths: Some(2),
            assists: Some(7),
            arranged_match: Some(2),
            map: "caldavar",
            mode: "banningpick",
            match_started: None,
            ..view()
        };
        let a = build(&v, &cfg()).unwrap();
        assert_eq!(a.details, "Ranked \u{2022} FoC \u{2022} Banning Pick");
        assert_eq!(a.state, "Pyromancer \u{2022} Lv 12 \u{2022} 4/2/7");
        assert!(a.start.is_none()); // timer off by default
    }

    #[test]
    fn timer_and_long_maps_optional() {
        let v = View {
            in_game: true,
            hero: "Hero_Pyromancer",
            arranged_match: Some(2),
            map: "caldavar",
            mode: "banningpick",
            match_started: Some(1000),
            ..view()
        };
        let mut c = cfg();
        c.show_timer = true;
        c.short_maps = false;
        let a = build(&v, &c).unwrap();
        assert_eq!(a.start, Some(1000));
        assert_eq!(
            a.details,
            "Ranked \u{2022} Forests of Caldavar \u{2022} Banning Pick"
        );
    }

    #[test]
    fn pregame_hero_select() {
        let v = View {
            game_phase: Some(2),
            local_client: Some(0),
            arranged_match: Some(2),
            map: "caldavar",
            mode: "allpick",
            ..view()
        };
        assert!(in_pregame(&v));
        let a = build(&v, &cfg()).unwrap();
        assert_eq!(a.details, "Ranked \u{2022} FoC \u{2022} All Pick");
        assert_eq!(a.state, "Hero Select");
    }

    #[test]
    fn panels() {
        for (panel, label) in [
            ("plinko_v2", "Playing Plinko"),
            ("store", "Browsing the Store"),
            ("learn", "In the Learn tab"),
            ("motd", "Main Menu"),
        ] {
            let v = View {
                panel,
                account: "x",
                ..view()
            };
            assert_eq!(build(&v, &cfg()).unwrap().details, label, "{panel}");
        }
    }

    #[test]
    fn spectating() {
        let v = View {
            in_game: true,
            local_client: Some(-1),
            view_client: Some(3),
            view_hero: "Hero_MasterOfArms",
            map: "caldavar",
            arranged_match: Some(2),
            ..view()
        };
        assert!(is_spectating(&v));
        let a = build(&v, &cfg()).unwrap();
        assert_eq!(a.details, "Spectating \u{2022} FoC");
        assert_eq!(a.state, "Watching Master Of Arms");
    }

    #[test]
    fn practice_with_empty_local_hero_is_not_spectating() {
        let v = View {
            in_game: true,
            local_client: Some(0),
            view_client: Some(0),
            hero: "",
            view_hero: "Hero_Pyromancer",
            map: "caldavar",
            ..view()
        };
        assert!(!is_spectating(&v));
        let a = build(&v, &cfg()).unwrap();
        assert_eq!(a.details, "FoC");
        assert_eq!(a.state, "Pyromancer");
    }

    #[test]
    fn hero_names() {
        assert_eq!(hero_label("Hero_DrRepulsor"), "Dr Repulsor");
        assert_eq!(hero_label("Hero_MasterOfArms"), "Master Of Arms");
        assert_eq!(hero_label("Hero_Pyromancer"), "Pyromancer");
    }
}
