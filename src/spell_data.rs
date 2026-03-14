use std::collections::HashMap;
use anyhow::Result;
use quick_xml::events::Event;
use quick_xml::Reader;

#[derive(Debug, Clone)]
pub struct SpellDef {
    pub num: u32,
    pub name: String,
    pub spell_type: String,
    pub circle: String,
    pub availability: String,
    pub no_incant: bool,
    pub msgup: Option<String>,
    pub msgdn: Option<String>,
    pub mana_cost: Option<String>,
    pub spirit_cost: Option<String>,
    pub stamina_cost: Option<String>,
    pub duration_self: Option<String>,
    pub duration_target: Option<String>,
    pub stackable: bool,
    pub refreshable: bool,
    pub persist_on_death: bool,
}

pub struct SpellList {
    spells: Vec<SpellDef>,
    by_num: HashMap<u32, usize>,
    by_name: HashMap<String, usize>,
}

fn parse_spell_attrs(e: &quick_xml::events::BytesStart<'_>) -> Result<SpellDef> {
    let mut spell = SpellDef {
        num: 0,
        name: String::new(),
        spell_type: String::new(),
        circle: String::new(),
        availability: String::new(),
        no_incant: false,
        msgup: None,
        msgdn: None,
        mana_cost: None,
        spirit_cost: None,
        stamina_cost: None,
        duration_self: None,
        duration_target: None,
        stackable: false,
        refreshable: false,
        persist_on_death: false,
    };
    for attr in e.attributes().flatten() {
        let key = std::str::from_utf8(attr.key.as_ref())?;
        let val = attr.unescape_value()?.into_owned();
        match key {
            "num" => spell.num = val.parse().unwrap_or(0),
            "name" => spell.name = val,
            "type" => spell.spell_type = val,
            "circle" => spell.circle = val,
            "availability" => spell.availability = val,
            "no_incant" | "noincant" => spell.no_incant = val == "true" || val == "yes",
            "stackable" => spell.stackable = val == "true" || val == "yes",
            "refreshable" => spell.refreshable = val == "true" || val == "yes",
            "persist_on_death" => spell.persist_on_death = val == "true" || val == "yes",
            _ => {}
        }
    }
    Ok(spell)
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
        let mut current_element: Option<String> = None;

        loop {
            match reader.read_event() {
                Ok(Event::Start(ref e)) => {
                    let tag = std::str::from_utf8(e.name().as_ref())?.to_string();
                    match tag.as_str() {
                        "effect" => {
                            current_spell = Some(parse_spell_attrs(e)?);
                        }
                        "msgup" | "msgdn" | "duration" | "duration_self" | "duration_target"
                        | "mana_cost" | "spirit_cost" | "stamina_cost" => {
                            current_element = Some(tag);
                        }
                        _ => {}
                    }
                }
                Ok(Event::Empty(ref e)) => {
                    let tag = std::str::from_utf8(e.name().as_ref())?.to_string();
                    if tag == "effect" {
                        // Self-closing <effect .../> — no End event follows, push immediately.
                        let spell = parse_spell_attrs(e)?;
                        spells.push(spell);
                    }
                }
                Ok(Event::Text(ref e)) => {
                    if let (Some(ref mut spell), Some(ref elem)) = (&mut current_spell, &current_element) {
                        let text = e.decode()?.into_owned();
                        match elem.as_str() {
                            "msgup" => spell.msgup = Some(text),
                            "msgdn" => spell.msgdn = Some(text),
                            "duration" | "duration_self" => spell.duration_self = Some(text),
                            "duration_target" => spell.duration_target = Some(text),
                            "mana_cost" => spell.mana_cost = Some(text),
                            "spirit_cost" => spell.spirit_cost = Some(text),
                            "stamina_cost" => spell.stamina_cost = Some(text),
                            _ => {}
                        }
                    }
                }
                Ok(Event::End(ref e)) => {
                    let name_bytes = e.name();
                    let tag = std::str::from_utf8(name_bytes.as_ref())?;
                    match tag {
                        "effect" => {
                            if let Some(spell) = current_spell.take() {
                                spells.push(spell);
                            }
                            current_element = None;
                        }
                        _ => {
                            current_element = None;
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
/// 3-digit (100-999): circle = num / 100 (e.g., 101 → 1, 901 → 9)
/// 4-digit (1000-9999): circle = num / 100 (e.g., 1700 → 17, 1005 → 10)
pub fn spell_circle(num: u32) -> u32 {
    num / 100
}

/// Circle number → human-readable name.
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

/// Circle number → char_data key suffix for spell ranks.
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
        let spell = SpellDef {
            num: 101, name: "Spirit Warding I".to_string(),
            spell_type: "defense".to_string(), circle: "1".to_string(),
            availability: "all".to_string(), no_incant: false,
            msgup: None, msgdn: None, mana_cost: None,
            spirit_cost: None, stamina_cost: None,
            duration_self: None, duration_target: None,
            stackable: false, refreshable: true, persist_on_death: false,
        };

        let mut ranks = HashMap::new();
        ranks.insert("minor_spiritual".to_string(), 20i64);

        // Spell 101 = circle 1, spell_in_circle = 1, effective = min(20, 100) = 20 → 1 <= 20 → known
        assert!(is_known(&spell, &ranks, 100));

        // Level 0 → effective = 0 → 1 <= 0 → not known
        assert!(!is_known(&spell, &ranks, 0));

        // No ranks in that circle
        let empty = HashMap::new();
        assert!(!is_known(&spell, &empty, 100));
    }

    #[test]
    fn test_parse_xml() {
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
}
