use std::collections::HashMap;
use anyhow::Result;
use quick_xml::events::Event;
use quick_xml::Reader;

#[derive(Debug, Clone)]
pub struct DurationDef {
    pub formula: String,
    pub stackable: bool,
    pub refreshable: bool,
    pub multicastable: bool,
    pub max_duration: f64,
    pub real_time: bool,
}

#[derive(Debug, Clone)]
pub struct SpellDef {
    pub num: u32,
    pub name: String,
    pub spell_type: String,
    pub circle: String,
    pub availability: String,
    pub no_incant: bool,
    pub stance: bool,
    pub channel: bool,
    pub msgup: Option<String>,
    pub msgdn: Option<String>,
    pub cost: HashMap<String, HashMap<String, String>>,  // cost_type -> {cast_type -> formula}
    pub duration: HashMap<String, DurationDef>,           // cast_type -> def
    pub bonus: HashMap<String, String>,                   // bonus_type -> formula
    pub cast_proc: Option<String>,
    pub stackable: bool,
    pub refreshable: bool,
    pub multicastable: bool,
    pub real_time: bool,
    pub max_duration: f64,
    pub persist_on_death: bool,
}

impl SpellDef {
    fn new() -> Self {
        SpellDef {
            num: 0,
            name: String::new(),
            spell_type: String::new(),
            circle: String::new(),
            availability: String::new(),
            no_incant: false,
            stance: false,
            channel: false,
            msgup: None,
            msgdn: None,
            cost: HashMap::new(),
            duration: HashMap::new(),
            bonus: HashMap::new(),
            cast_proc: None,
            stackable: false,
            refreshable: false,
            multicastable: false,
            real_time: false,
            max_duration: 0.0,
            persist_on_death: false,
        }
    }

    /// Get mana cost formula for self-cast (backward compat).
    pub fn mana_cost(&self) -> Option<&str> {
        self.cost.get("mana")?.get("self").map(|s| s.as_str())
    }

    pub fn spirit_cost(&self) -> Option<&str> {
        self.cost.get("spirit")?.get("self").map(|s| s.as_str())
    }

    pub fn stamina_cost(&self) -> Option<&str> {
        self.cost.get("stamina")?.get("self").map(|s| s.as_str())
    }

    pub fn duration_self(&self) -> Option<&str> {
        self.duration.get("self").map(|d| d.formula.as_str())
    }

    pub fn duration_target(&self) -> Option<&str> {
        self.duration.get("target").map(|d| d.formula.as_str())
    }
}

pub struct SpellList {
    spells: Vec<SpellDef>,
    by_num: HashMap<u32, usize>,
    by_name: HashMap<String, usize>,
}

/// Temporary state for tracking attributes on the current child element being parsed.
struct ChildElementState {
    tag: String,
    /// Attributes collected from the element's start tag.
    attrs: HashMap<String, String>,
}

fn parse_spell_attrs(e: &quick_xml::events::BytesStart<'_>) -> Result<SpellDef> {
    let mut spell = SpellDef::new();
    for attr in e.attributes().flatten() {
        let key = std::str::from_utf8(attr.key.as_ref())?;
        let val = attr.unescape_value()?.into_owned();
        match key {
            "num" | "number" => spell.num = val.parse().unwrap_or(0),
            "name" => spell.name = val,
            "type" => spell.spell_type = val,
            "circle" => spell.circle = val,
            "availability" => spell.availability = val,
            "no_incant" | "noincant" => spell.no_incant = val == "true" || val == "yes",
            "stance" => spell.stance = val == "true" || val == "yes",
            "channel" => spell.channel = val == "true" || val == "yes",
            "stackable" => spell.stackable = val == "true" || val == "yes",
            "refreshable" => spell.refreshable = val == "true" || val == "yes",
            "persist_on_death" => spell.persist_on_death = val == "true" || val == "yes",
            _ => {}
        }
    }
    Ok(spell)
}

/// Collect attributes from a child element start tag into a HashMap.
fn collect_attrs(e: &quick_xml::events::BytesStart<'_>) -> Result<HashMap<String, String>> {
    let mut map = HashMap::new();
    for attr in e.attributes().flatten() {
        let key = std::str::from_utf8(attr.key.as_ref())?.to_string();
        let val = attr.unescape_value()?.into_owned();
        map.insert(key, val);
    }
    Ok(map)
}

/// Apply a child element's text content to the spell, using the element tag and attributes.
fn apply_child_text(spell: &mut SpellDef, child: &ChildElementState, text: &str) {
    match child.tag.as_str() {
        // Old format elements
        "msgup" => spell.msgup = Some(text.to_string()),
        "msgdn" => spell.msgdn = Some(text.to_string()),
        "duration" | "duration_self" => {
            spell.duration.entry("self".to_string()).or_insert_with(|| DurationDef {
                formula: text.to_string(),
                stackable: spell.stackable,
                refreshable: spell.refreshable,
                multicastable: false,
                max_duration: 0.0,
                real_time: false,
            });
        }
        "duration_target" => {
            spell.duration.entry("target".to_string()).or_insert_with(|| DurationDef {
                formula: text.to_string(),
                stackable: spell.stackable,
                refreshable: spell.refreshable,
                multicastable: false,
                max_duration: 0.0,
                real_time: false,
            });
        }
        "mana_cost" => {
            spell.cost.entry("mana".to_string())
                .or_default()
                .insert("self".to_string(), text.to_string());
        }
        "spirit_cost" => {
            spell.cost.entry("spirit".to_string())
                .or_default()
                .insert("self".to_string(), text.to_string());
        }
        "stamina_cost" => {
            spell.cost.entry("stamina".to_string())
                .or_default()
                .insert("self".to_string(), text.to_string());
        }
        // Lich5 format elements
        "message" => {
            let msg_type = child.attrs.get("type").map(|s| s.as_str()).unwrap_or("");
            match msg_type {
                "start" => spell.msgup = Some(text.to_string()),
                "end" => spell.msgdn = Some(text.to_string()),
                _ => {}
            }
        }
        "cost" => {
            let cost_type = child.attrs.get("type").cloned().unwrap_or_default();
            let cast_type = child.attrs.get("cast-type").cloned().unwrap_or_else(|| "self".to_string());
            if !cost_type.is_empty() {
                spell.cost.entry(cost_type)
                    .or_default()
                    .insert(cast_type, text.to_string());
            }
        }
        "bonus" => {
            let bonus_type = child.attrs.get("type").cloned().unwrap_or_default();
            if !bonus_type.is_empty() {
                spell.bonus.insert(bonus_type, text.to_string());
            }
        }
        "cast-proc" => {
            spell.cast_proc = Some(text.to_string());
        }
        _ => {}
    }
}

/// Apply a Lich5 `<duration>` element (which has attributes) when its text content arrives.
fn apply_duration_text(spell: &mut SpellDef, attrs: &HashMap<String, String>, text: &str) {
    let cast_type = attrs.get("cast-type").cloned().unwrap_or_else(|| "self".to_string());
    let span = attrs.get("span").map(|s| s.as_str()).unwrap_or("");
    let stackable = span == "stackable";
    let refreshable = span == "refreshable";
    let multicastable = attrs.get("multicastable").map(|v| v == "yes" || v == "true").unwrap_or(false);
    let persist = attrs.get("persist-on-death").map(|v| v == "yes" || v == "true").unwrap_or(false);
    let real_time = attrs.get("real-time").map(|v| v == "yes" || v == "true").unwrap_or(false);
    let max_dur: f64 = attrs.get("max").and_then(|v| v.parse().ok()).unwrap_or(0.0);

    let is_first = spell.duration.is_empty();

    spell.duration.insert(cast_type, DurationDef {
        formula: text.to_string(),
        stackable,
        refreshable,
        multicastable,
        max_duration: max_dur,
        real_time,
    });

    // Set top-level flags from the first duration encountered.
    if is_first {
        spell.stackable = stackable;
        spell.refreshable = refreshable;
        spell.multicastable = multicastable;
        spell.persist_on_death = persist;
        spell.real_time = real_time;
        spell.max_duration = max_dur;
    }
}

impl SpellList {
    /// Load spell definitions from an effect-list.xml file.
    pub fn load(path: &str) -> Result<Self> {
        let content = std::fs::read_to_string(path)?;
        Self::parse(&content)
    }

    /// Parse spell definitions from XML string (for testing).
    pub fn parse(xml: &str) -> Result<Self> {
        let mut reader = Reader::from_str(xml);
        reader.config_mut().trim_text(true);

        let mut spells = Vec::new();
        let mut current_spell: Option<SpellDef> = None;
        let mut current_child: Option<ChildElementState> = None;

        loop {
            match reader.read_event() {
                Ok(Event::Start(ref e)) => {
                    let tag = std::str::from_utf8(e.name().as_ref())?.to_string();
                    match tag.as_str() {
                        "effect" | "spell" => {
                            current_spell = Some(parse_spell_attrs(e)?);
                        }
                        _ if current_spell.is_some() => {
                            let attrs = collect_attrs(e)?;
                            current_child = Some(ChildElementState { tag, attrs });
                        }
                        _ => {}
                    }
                }
                Ok(Event::Empty(ref e)) => {
                    let tag = std::str::from_utf8(e.name().as_ref())?.to_string();
                    match tag.as_str() {
                        "effect" | "spell" => {
                            let spell = parse_spell_attrs(e)?;
                            spells.push(spell);
                        }
                        _ => {}
                    }
                }
                Ok(Event::Text(ref e)) => {
                    if let (Some(ref mut spell), Some(ref child)) = (&mut current_spell, &current_child) {
                        let text = e.decode()?.into_owned();
                        // Duration elements in Lich5 format carry attrs that affect parsing.
                        if child.tag == "duration" && !child.attrs.is_empty() {
                            apply_duration_text(spell, &child.attrs, &text);
                        } else {
                            apply_child_text(spell, child, &text);
                        }
                    }
                }
                Ok(Event::End(ref e)) => {
                    let name_bytes = e.name();
                    let tag = std::str::from_utf8(name_bytes.as_ref())?;
                    match tag {
                        "effect" | "spell" => {
                            if let Some(spell) = current_spell.take() {
                                spells.push(spell);
                            }
                            current_child = None;
                        }
                        _ => {
                            current_child = None;
                        }
                    }
                }
                Ok(Event::Eof) => break,
                Err(e) => {
                    tracing::warn!("effect-list.xml parse error: {e}");
                    break;
                }
                _ => {}
            }
        }

        let mut by_num = HashMap::new();
        let mut by_name = HashMap::new();
        for (i, spell) in spells.iter().enumerate() {
            by_num.insert(spell.num, i);
            by_name.insert(spell.name.to_lowercase(), i);
        }

        Ok(Self { spells, by_num, by_name })
    }

    pub fn get_by_num(&self, num: u32) -> Option<&SpellDef> {
        self.by_num.get(&num).map(|&i| &self.spells[i])
    }

    pub fn get_by_name(&self, name: &str) -> Option<&SpellDef> {
        self.by_name.get(&name.to_lowercase()).map(|&i| &self.spells[i])
    }

    pub fn all(&self) -> &[SpellDef] {
        &self.spells
    }

    pub fn len(&self) -> usize {
        self.spells.len()
    }

    pub fn is_empty(&self) -> bool {
        self.spells.is_empty()
    }
}

/// Compute spell circle number from spell number.
/// 3-digit (100-999): circle = num / 100 (e.g., 101 -> 1, 901 -> 9)
/// 4-digit (1000-9999): circle = num / 100 (e.g., 1700 -> 17, 1005 -> 10)
pub fn spell_circle(num: u32) -> u32 {
    num / 100
}

/// Circle number -> human-readable name.
pub fn circle_name(circle: u32) -> &'static str {
    match circle {
        1  => "Minor Spirit",
        2  => "Major Spirit",
        3  => "Cleric",
        4  => "Minor Elemental",
        5  => "Major Elemental",
        6  => "Ranger",
        7  => "Sorcerer",
        9  => "Wizard",
        10 => "Bard",
        11 => "Empath",
        12 => "Minor Mental",
        16 => "Paladin",
        17 => "Arcane",
        _  => "Unknown",
    }
}

/// Circle number -> char_data key suffix for spell ranks.
pub fn circle_data_key(circle: u32) -> Option<&'static str> {
    match circle {
        1  => Some("minor_spiritual"),
        2  => Some("major_spiritual"),
        3  => Some("cleric"),
        4  => Some("minor_elemental"),
        5  => Some("major_elemental"),
        6  => Some("ranger"),
        7  => Some("sorcerer"),
        9  => Some("wizard"),
        10 => Some("bard"),
        11 => Some("empath"),
        12 => Some("minor_mental"),
        16 => Some("paladin"),
        _  => None,
    }
}

/// Check if a spell is known based on circle ranks and character level.
pub fn is_known(spell: &SpellDef, circle_ranks: &HashMap<String, i64>, level: u32) -> bool {
    let circle = spell_circle(spell.num);
    let circle_key = match circle_data_key(circle) {
        Some(k) => k,
        None => return false, // Arcane (17), society (96-99), unknown circles
    };
    let ranks = circle_ranks.get(circle_key).copied().unwrap_or(0);
    let effective = ranks.min(level as i64);
    let spell_in_circle = (spell.num % 100) as i64;
    spell_in_circle <= effective
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_spell_circle() {
        assert_eq!(spell_circle(101), 1);
        assert_eq!(spell_circle(199), 1);
        assert_eq!(spell_circle(901), 9);
        assert_eq!(spell_circle(1700), 17);
        assert_eq!(spell_circle(1005), 10);
    }

    #[test]
    fn test_circle_name() {
        assert_eq!(circle_name(1), "Minor Spirit");
        assert_eq!(circle_name(9), "Wizard");
        assert_eq!(circle_name(17), "Arcane");
    }

    #[test]
    fn test_is_known() {
        let mut spell = SpellDef::new();
        spell.num = 101;
        spell.name = "Spirit Warding I".to_string();
        spell.spell_type = "defense".to_string();
        spell.circle = "1".to_string();
        spell.availability = "all".to_string();
        spell.refreshable = true;

        let mut ranks = HashMap::new();
        ranks.insert("minor_spiritual".to_string(), 20i64);

        // Spell 101 = circle 1, spell_in_circle = 1, effective = min(20, 100) = 20 -> 1 <= 20 -> known
        assert!(is_known(&spell, &ranks, 100));

        // Level 0 -> effective = 0 -> 1 <= 0 -> not known
        assert!(!is_known(&spell, &ranks, 0));

        // No ranks in that circle
        let empty = HashMap::new();
        assert!(!is_known(&spell, &empty, 100));
    }

    #[test]
    fn test_parse_xml_old_format() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<effects>
  <effect num="101" name="Spirit Warding I" type="defense" circle="1"
          availability="all" stackable="false" refreshable="true"
          persist_on_death="false">
    <msgup>The dim aura fades from around you.</msgup>
    <msgdn>The bright aura fades from around you.</msgdn>
  </effect>
  <effect num="901" name="Wizard Shield" type="defense" circle="9"
          availability="self-cast" stackable="false" refreshable="true"
          persist_on_death="false">
  </effect>
</effects>"#;

        let sl = SpellList::parse(xml).unwrap();
        assert_eq!(sl.len(), 2);

        let s101 = sl.get_by_num(101).unwrap();
        assert_eq!(s101.name, "Spirit Warding I");
        assert_eq!(s101.spell_type, "defense");
        assert!(s101.msgup.is_some());

        let s901 = sl.get_by_name("Wizard Shield").unwrap();
        assert_eq!(s901.num, 901);
    }

    #[test]
    fn test_parse_xml_old_format_costs_and_durations() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<effects>
  <effect num="101" name="Spirit Warding I" type="defense" circle="1"
          availability="all" stackable="false" refreshable="true"
          persist_on_death="false">
    <mana_cost>1</mana_cost>
    <duration_self>1200</duration_self>
    <duration_target>600</duration_target>
    <msgup>You feel warded.</msgup>
    <msgdn>The ward fades.</msgdn>
  </effect>
</effects>"#;

        let sl = SpellList::parse(xml).unwrap();
        let s = sl.get_by_num(101).unwrap();
        assert_eq!(s.mana_cost(), Some("1"));
        assert_eq!(s.spirit_cost(), None);
        assert_eq!(s.duration_self(), Some("1200"));
        assert_eq!(s.duration_target(), Some("600"));
    }

    #[test]
    fn test_parse_xml_lich5_format() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<list>
  <spell availability='all' name='Spirit Warding I' number='101' type='defense'>
    <duration cast-type='self' span='stackable' multicastable='yes'>1200</duration>
    <duration cast-type='target' span='stackable' multicastable='yes'>600</duration>
    <cost type='mana'>1</cost>
    <cost type='mana' cast-type='target'>2</cost>
    <bonus type='bolt-ds'>10</bonus>
    <bonus type='spiritual-td'>5</bonus>
    <message type='start'>You feel warded.</message>
    <message type='end'>The ward fades.</message>
    <cast-proc>some_proc</cast-proc>
  </spell>
  <spell availability='self-cast' name='Wizard Shield' number='901' type='defense'>
    <duration span='refreshable' persist-on-death='yes' real-time='yes' max='3600'>900</duration>
    <cost type='mana'>9</cost>
  </spell>
</list>"#;

        let sl = SpellList::parse(xml).unwrap();
        assert_eq!(sl.len(), 2);

        let s101 = sl.get_by_num(101).unwrap();
        assert_eq!(s101.name, "Spirit Warding I");
        assert_eq!(s101.spell_type, "defense");
        assert_eq!(s101.availability, "all");

        // Costs
        assert_eq!(s101.mana_cost(), Some("1"));
        assert_eq!(s101.cost.get("mana").unwrap().get("target").map(|s| s.as_str()), Some("2"));

        // Durations
        assert_eq!(s101.duration_self(), Some("1200"));
        assert_eq!(s101.duration_target(), Some("600"));
        let dur_self = s101.duration.get("self").unwrap();
        assert!(dur_self.stackable);
        assert!(dur_self.multicastable);

        // Top-level flags from first duration
        assert!(s101.stackable);
        assert!(s101.multicastable);

        // Bonuses
        assert_eq!(s101.bonus.get("bolt-ds").map(|s| s.as_str()), Some("10"));
        assert_eq!(s101.bonus.get("spiritual-td").map(|s| s.as_str()), Some("5"));

        // Messages
        assert_eq!(s101.msgup.as_deref(), Some("You feel warded."));
        assert_eq!(s101.msgdn.as_deref(), Some("The ward fades."));

        // Cast proc
        assert_eq!(s101.cast_proc.as_deref(), Some("some_proc"));

        // Spell 901 — duration with no cast-type defaults to "self"
        let s901 = sl.get_by_num(901).unwrap();
        assert_eq!(s901.name, "Wizard Shield");
        assert_eq!(s901.mana_cost(), Some("9"));
        assert_eq!(s901.duration_self(), Some("900"));
        let dur = s901.duration.get("self").unwrap();
        assert!(!dur.stackable);
        assert!(dur.refreshable);
        assert!(dur.real_time);
        assert_eq!(dur.max_duration, 3600.0);
        // Top-level flags
        assert!(s901.persist_on_death);
        assert!(s901.real_time);
        assert_eq!(s901.max_duration, 3600.0);
    }

    #[test]
    fn test_convenience_methods() {
        let mut spell = SpellDef::new();
        spell.cost.entry("mana".to_string()).or_default().insert("self".to_string(), "5".to_string());
        spell.cost.entry("spirit".to_string()).or_default().insert("self".to_string(), "2".to_string());
        spell.duration.insert("self".to_string(), DurationDef {
            formula: "1200".to_string(),
            stackable: false,
            refreshable: true,
            multicastable: false,
            max_duration: 0.0,
            real_time: false,
        });
        spell.duration.insert("target".to_string(), DurationDef {
            formula: "600".to_string(),
            stackable: false,
            refreshable: true,
            multicastable: false,
            max_duration: 0.0,
            real_time: false,
        });

        assert_eq!(spell.mana_cost(), Some("5"));
        assert_eq!(spell.spirit_cost(), Some("2"));
        assert_eq!(spell.stamina_cost(), None);
        assert_eq!(spell.duration_self(), Some("1200"));
        assert_eq!(spell.duration_target(), Some("600"));
    }
}
