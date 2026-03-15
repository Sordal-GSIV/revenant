use std::collections::HashMap;
use std::sync::LazyLock;
use regex::Regex;
use crate::db::Db;

// ── Regex Patterns ──────────────────────────────────────────────────────────

// INFO command output
static CHAR_RACE_PROF: LazyLock<Regex> = LazyLock::new(|| Regex::new(
    r"^Name:\s+[\w\s'-]+\s+Race:\s+([\w -]+)\s+Profession:\s+([-\w]+)"
).unwrap());

static CHAR_GENDER_AGE: LazyLock<Regex> = LazyLock::new(|| Regex::new(
    r"^Gender:\s+(\w+)\s+Age:\s+([\d,]+)"
).unwrap());

// Matches both 'info' (2 columns) and 'info full' (3 columns with base stats)
// Groups: 1=stat name, 2=base_val?, 3=base_bonus?, 4=current_val, 5=current_bonus, 6=enhanced_val, 7=enhanced_bonus
static STAT_LINE: LazyLock<Regex> = LazyLock::new(|| Regex::new(
    r"^\s*(\w+)\s\((?:STR|CON|DEX|AGI|DIS|AUR|LOG|INT|WIS|INF)\):(?:\s+(\d+)\s+\((-?\d+)\)\s+\.{3})?\s+(\d+)\s+\((-?\d+)\)\s+\.{3}\s+(\d+)\s+\((-?\d+)\)"
).unwrap());

static STAT_END: LazyLock<Regex> = LazyLock::new(|| Regex::new(
    r"^Mana:\s+-?\d+\s+Silver:"
).unwrap());

// SKILLS command output
static SKILL_START: LazyLock<Regex> = LazyLock::new(|| Regex::new(
    r"^\s\w+\s\(at level \d+\).*skill bonuses and ranks"
).unwrap());

// Skill line: name | bonus ranks
static SKILL_LINE: LazyLock<Regex> = LazyLock::new(|| Regex::new(
    r"^\s+([\w\s\-']+?)\.+\|\s+(\d+)\s+(\d+)"
).unwrap());

// Spell rank line inside SKILLS output: name | ranks (no bonus column)
static SPELL_RANK_LINE: LazyLock<Regex> = LazyLock::new(|| Regex::new(
    r"^\s+([\w\s\-']+?)\.+\|\s+(\d+)\s*$"
).unwrap());

static SKILL_END: LazyLock<Regex> = LazyLock::new(|| Regex::new(
    r"^Training Points:\s+\d+\s+Phy\s+\d+\s+Mnt"
).unwrap());

// SPELL command (standalone, outside SKILLS block)
static SPELL_SOLO: LazyLock<Regex> = LazyLock::new(|| Regex::new(
    r"^(Bard|Cleric|Empath|Minor (?:Elemental|Mental|Spiritual)|Major (?:Elemental|Mental|Spiritual)|Paladin|Ranger|Savant|Sorcerer|Wizard)(?: Base)?\.+(\d+)"
).unwrap());

// ── Key Normalization ───────────────────────────────────────────────────────

fn normalize_key(raw: &str) -> String {
    raw.trim()
        .to_lowercase()
        .replace([' ', '-'], "_")
        .replace("__", "_")
}

// ── Parser State ────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
enum ParserState {
    Ready,
    InStats,
    InSkills,
}

/// Infomon-style verb output parser. Parses INFO/SKILLS/SPELL command output
/// from the game text stream and writes parsed key-value pairs to the char_data cache.
pub struct Infomon {
    db: Db,
    character: String,
    game: String,
    state: ParserState,
    batch: Vec<(String, String)>,
    cache: HashMap<String, String>,
}

impl Infomon {
    /// Create a new Infomon. Loads existing char_data from DB into cache.
    pub fn new(db: Db, character: &str, game: &str) -> Self {
        let mut cache = HashMap::new();
        // Load existing data from DB into cache
        if let Ok(pairs) = db.get_all_char_data(character, game) {
            for (k, v) in pairs {
                cache.insert(k, v);
            }
        }
        Self {
            db,
            character: character.to_string(),
            game: game.to_string(),
            state: ParserState::Ready,
            batch: Vec::new(),
            cache,
        }
    }

    /// Read a cached value by key. Returns None if not found.
    pub fn get(&self, key: &str) -> Option<&str> {
        self.cache.get(key).map(|s| s.as_str())
    }

    /// Read a cached value as i64. Returns 0 if missing or unparseable.
    pub fn get_i64(&self, key: &str) -> i64 {
        self.cache.get(key)
            .and_then(|v| v.parse::<i64>().ok())
            .unwrap_or(0)
    }

    /// Get all keys matching a prefix (e.g., "stat.", "skill.", "spell.").
    pub fn get_prefix(&self, prefix: &str) -> Vec<(&str, &str)> {
        self.cache.iter()
            .filter(|(k, _)| k.starts_with(prefix))
            .map(|(k, v)| (k.as_str(), v.as_str()))
            .collect()
    }

    /// Parse a single line of game output. Called by the downstream hook.
    pub fn parse(&mut self, line: &str) {
        match self.state {
            ParserState::Ready => self.parse_ready(line),
            ParserState::InStats => self.parse_in_stats(line),
            ParserState::InSkills => self.parse_in_skills(line),
        }
    }

    fn parse_ready(&mut self, line: &str) {
        // Check for INFO header: "Name: ... Race: ... Profession: ..."
        if let Some(caps) = CHAR_RACE_PROF.captures(line) {
            self.state = ParserState::InStats;
            self.batch.clear();
            let race = caps.get(1).map(|m| m.as_str().trim()).unwrap_or("");
            let prof = caps.get(2).map(|m| m.as_str().trim()).unwrap_or("");
            self.batch.push(("stat.race".to_string(), race.to_string()));
            self.batch.push(("stat.profession".to_string(), prof.to_string()));
            return;
        }

        // Check for SKILLS header
        if SKILL_START.is_match(line) {
            self.state = ParserState::InSkills;
            self.batch.clear();
            return;
        }

        // Check for standalone SPELL line
        if let Some(caps) = SPELL_SOLO.captures(line) {
            let circle_name = caps.get(1).map(|m| m.as_str()).unwrap_or("");
            let ranks = caps.get(2).map(|m| m.as_str()).unwrap_or("0");
            let key = format!("spell.{}", normalize_key(circle_name));
            self.cache.insert(key.clone(), ranks.to_string());
            let _ = self.db.set_char_data(&self.character, &self.game, &key, ranks);
            return;
        }
    }

    fn parse_in_stats(&mut self, line: &str) {
        // Check for gender/age line
        if let Some(caps) = CHAR_GENDER_AGE.captures(line) {
            let gender = caps.get(1).map(|m| m.as_str()).unwrap_or("");
            let age = caps.get(2).map(|m| m.as_str().replace(',', "")).unwrap_or_default();
            self.batch.push(("stat.gender".to_string(), gender.to_string()));
            self.batch.push(("stat.age".to_string(), age));
            return;
        }

        // Check for stat line (e.g., "  Strength (STR):   87  (12) ...   92  (16)")
        if let Some(caps) = STAT_LINE.captures(line) {
            let name = normalize_key(caps.get(1).map(|m| m.as_str()).unwrap_or(""));

            // Current value (group 4) and bonus (group 5) — always present
            let cur_val = caps.get(4).map(|m| m.as_str()).unwrap_or("0");
            let cur_bonus = caps.get(5).map(|m| m.as_str()).unwrap_or("0");
            self.batch.push((format!("stat.{name}"), cur_val.to_string()));
            self.batch.push((format!("stat.{name}_bonus"), cur_bonus.to_string()));

            // Enhanced value (group 6) and bonus (group 7) — always present
            let enh_val = caps.get(6).map(|m| m.as_str()).unwrap_or("0");
            let enh_bonus = caps.get(7).map(|m| m.as_str()).unwrap_or("0");
            self.batch.push((format!("stat.{name}.enhanced"), enh_val.to_string()));
            self.batch.push((format!("stat.{name}.enhanced_bonus"), enh_bonus.to_string()));

            // Base value (group 2) and bonus (group 3) — only present with "info full"
            if let (Some(base_v), Some(base_b)) = (caps.get(2), caps.get(3)) {
                self.batch.push((format!("stat.{name}.base"), base_v.as_str().to_string()));
                self.batch.push((format!("stat.{name}.base_bonus"), base_b.as_str().to_string()));
            }
            return;
        }

        // Check for stat block end: "Mana: ... Silver: ..."
        if STAT_END.is_match(line) {
            self.flush_batch();
            self.state = ParserState::Ready;
            return;
        }
    }

    fn parse_in_skills(&mut self, line: &str) {
        // Check for skill line: "  Edged Weapons..........|  140   30"
        if let Some(caps) = SKILL_LINE.captures(line) {
            let name = normalize_key(caps.get(1).map(|m| m.as_str()).unwrap_or(""));
            let bonus = caps.get(2).map(|m| m.as_str()).unwrap_or("0");
            let ranks = caps.get(3).map(|m| m.as_str()).unwrap_or("0");
            self.batch.push((format!("skill.{name}"), ranks.to_string()));
            self.batch.push((format!("skill.{name}_bonus"), bonus.to_string()));
            return;
        }

        // Check for spell rank line inside SKILLS: "  Minor Elemental........|   30"
        if let Some(caps) = SPELL_RANK_LINE.captures(line) {
            let name = normalize_key(caps.get(1).map(|m| m.as_str()).unwrap_or(""));
            let ranks = caps.get(2).map(|m| m.as_str()).unwrap_or("0");
            self.batch.push((format!("spell.{name}"), ranks.to_string()));
            return;
        }

        // Check for skills block end: "Training Points: 0 Phy 0 Mnt"
        if SKILL_END.is_match(line) {
            self.flush_batch();
            self.state = ParserState::Ready;
            return;
        }
    }

    /// Flush accumulated batch to DB and cache.
    fn flush_batch(&mut self) {
        if self.batch.is_empty() {
            return;
        }
        // Update in-memory cache
        for (k, v) in &self.batch {
            self.cache.insert(k.clone(), v.clone());
        }
        // Write to DB in single transaction
        let pairs: Vec<(&str, &str)> = self.batch.iter()
            .map(|(k, v)| (k.as_str(), v.as_str()))
            .collect();
        if let Err(e) = self.db.set_char_data_batch(&self.character, &self.game, &pairs) {
            tracing::warn!("Infomon: failed to write batch to DB: {e}");
        }
        self.batch.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_infomon() -> Infomon {
        let db = Db::open(":memory:").unwrap();
        Infomon::new(db, "Ondreian", "GS3")
    }

    #[test]
    fn test_normalize_key() {
        assert_eq!(normalize_key("Edged Weapons"), "edged_weapons");
        assert_eq!(normalize_key("Two Weapon Combat"), "two_weapon_combat");
        assert_eq!(normalize_key("Stalking and Hiding"), "stalking_and_hiding");
        assert_eq!(normalize_key("Minor Elemental"), "minor_elemental");
    }

    #[test]
    fn test_parse_info_output() {
        let mut im = test_infomon();

        // Simulated INFO output
        im.parse("Name: Ondreian O'Something   Race: Human   Profession: Wizard");
        assert_eq!(im.state, ParserState::InStats);

        im.parse("Gender: male   Age: 247");

        // Stat line (2-column format: current + enhanced, no base)
        im.parse("  Strength (STR):                    87  (12) ...   92  (16)");
        im.parse("  Constitution (CON):                80  (10) ...   85  (12)");

        // End of stats block
        im.parse("Mana: 150   Silver: 1234");
        assert_eq!(im.state, ParserState::Ready);

        // Verify cached values
        assert_eq!(im.get("stat.race"), Some("Human"));
        assert_eq!(im.get("stat.profession"), Some("Wizard"));
        assert_eq!(im.get("stat.gender"), Some("male"));
        assert_eq!(im.get("stat.age"), Some("247"));
        assert_eq!(im.get_i64("stat.strength"), 87);
        assert_eq!(im.get_i64("stat.strength_bonus"), 12);
        assert_eq!(im.get_i64("stat.strength.enhanced"), 92);
        assert_eq!(im.get_i64("stat.strength.enhanced_bonus"), 16);
    }

    #[test]
    fn test_parse_info_full_output() {
        let mut im = test_infomon();

        im.parse("Name: Ondreian   Race: Human   Profession: Wizard");
        // info full: 3-column format with base stats
        im.parse("  Strength (STR):   80  (10) ...   87  (12) ...   92  (16)");
        im.parse("Mana: 150   Silver: 1234");

        assert_eq!(im.get_i64("stat.strength"), 87);
        assert_eq!(im.get_i64("stat.strength_bonus"), 12);
        assert_eq!(im.get_i64("stat.strength.base"), 80);
        assert_eq!(im.get_i64("stat.strength.base_bonus"), 10);
        assert_eq!(im.get_i64("stat.strength.enhanced"), 92);
        assert_eq!(im.get_i64("stat.strength.enhanced_bonus"), 16);
    }

    #[test]
    fn test_parse_skills_output() {
        let mut im = test_infomon();

        im.parse(" Ondreian (at level 100) has earned 87,654,321 experience and has skill bonuses and ranks as follows:");
        assert_eq!(im.state, ParserState::InSkills);

        im.parse("   Armor Use....................|  140   30");
        im.parse("   Shield Use...................|   60   12");
        im.parse("   Edged Weapons................|  140   30");
        im.parse("   Two Weapon Combat............|  162   62");
        im.parse("   Stalking and Hiding..........|  162   62");

        // Spell ranks inside SKILLS block
        im.parse("   Minor Elemental..............|   30");
        im.parse("   Major Elemental..............|   25");

        im.parse("Training Points: 0 Phy 0 Mnt");
        assert_eq!(im.state, ParserState::Ready);

        assert_eq!(im.get_i64("skill.armor_use"), 30);
        assert_eq!(im.get_i64("skill.armor_use_bonus"), 140);
        assert_eq!(im.get_i64("skill.edged_weapons"), 30);
        assert_eq!(im.get_i64("skill.two_weapon_combat"), 62);
        assert_eq!(im.get_i64("skill.stalking_and_hiding"), 62);
        assert_eq!(im.get_i64("spell.minor_elemental"), 30);
        assert_eq!(im.get_i64("spell.major_elemental"), 25);
    }

    #[test]
    fn test_parse_standalone_spell() {
        let mut im = test_infomon();

        im.parse("Minor Elemental Base..............30");
        assert_eq!(im.get_i64("spell.minor_elemental"), 30);

        im.parse("Wizard Base.......................50");
        assert_eq!(im.get_i64("spell.wizard"), 50);

        im.parse("Major Spiritual...................15");
        assert_eq!(im.get_i64("spell.major_spiritual"), 15);
    }

    #[test]
    fn test_cache_persistence() {
        // Verify that data written during parsing persists via DB
        // Use a temp file since :memory: can't be shared across Db instances
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.db").to_string_lossy().to_string();
        {
            let db = Db::open(&path).unwrap();
            let mut im = Infomon::new(db, "Ondreian", "GS3");
            im.parse("Minor Elemental Base..............30");
        }
        // New Infomon instance loads from same DB
        let db2 = Db::open(&path).unwrap();
        let im2 = Infomon::new(db2, "Ondreian", "GS3");
        assert_eq!(im2.get_i64("spell.minor_elemental"), 30);
    }

    #[test]
    fn test_unrelated_lines_ignored() {
        let mut im = test_infomon();
        im.parse("You attack the kobold!");
        im.parse("A kobold swings at you with a rusty sword!");
        im.parse("");
        im.parse("Obvious exits: north, south");
        assert_eq!(im.state, ParserState::Ready);
        assert!(im.cache.is_empty());
    }

    #[test]
    fn test_get_prefix() {
        let mut im = test_infomon();
        im.parse("Minor Elemental Base..............30");
        im.parse("Wizard Base.......................50");

        let spells = im.get_prefix("spell.");
        assert_eq!(spells.len(), 2);
    }
}
