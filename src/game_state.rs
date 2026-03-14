use std::time::{Duration, Instant};
use crate::xml_parser::XmlEvent;

#[derive(Debug, Clone, PartialEq, Default)]
pub enum Game { #[default] GemStone, DragonRealms }

#[derive(Debug, Clone, PartialEq, Default)]
pub enum Stance { #[default] None, Offensive, Advance, Forward, Neutral, Guarded, Defensive }

#[derive(Debug, Clone, PartialEq, Default)]
pub enum MindState {
    #[default] Clear, Dabbling, Awakening, Thinking, Considering,
    Pondering, Ruminating, Focusing, Deliberating, Concentrating,
    Attentive, Distracted, Muddled, BecomingFuzzy, Fuzzy,
    SlightlyDizzy, Dizzy, VeryDizzy, Ropy, Stunned,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub enum EncumbranceState { #[default] None, Light, Moderate, Heavy, VeryHeavy, Overburdened }

impl Stance {
    pub fn to_str(&self) -> Option<&'static str> {
        match self {
            Stance::None => None,
            Stance::Offensive => Some("offensive"),
            Stance::Advance => Some("advance"),
            Stance::Forward => Some("forward"),
            Stance::Neutral => Some("neutral"),
            Stance::Guarded => Some("guarded"),
            Stance::Defensive => Some("defensive"),
        }
    }
    pub fn to_value(&self) -> Option<i64> {
        match self {
            Stance::None => None,
            Stance::Offensive => Some(100),
            Stance::Advance => Some(80),
            Stance::Forward => Some(60),
            Stance::Neutral => Some(40),
            Stance::Guarded => Some(20),
            Stance::Defensive => Some(0),
        }
    }
}

impl MindState {
    pub fn to_str(&self) -> &'static str {
        match self {
            MindState::Clear => "clear",
            MindState::Dabbling => "dabbling",
            MindState::Awakening => "awakening",
            MindState::Thinking => "thinking",
            MindState::Considering => "considering",
            MindState::Pondering => "pondering",
            MindState::Ruminating => "ruminating",
            MindState::Focusing => "focusing",
            MindState::Deliberating => "deliberating",
            MindState::Concentrating => "concentrating",
            MindState::Attentive => "attentive",
            MindState::Distracted => "distracted",
            MindState::Muddled => "muddled",
            MindState::BecomingFuzzy => "becoming fuzzy",
            MindState::Fuzzy => "fuzzy",
            MindState::SlightlyDizzy => "slightly dizzy",
            MindState::Dizzy => "dizzy",
            MindState::VeryDizzy => "very dizzy",
            MindState::Ropy => "ropy",
            MindState::Stunned => "stunned",
        }
    }
    pub fn to_value(&self) -> i64 {
        match self {
            MindState::Clear => 0,
            MindState::Dabbling => 5,
            MindState::Awakening => 10,
            MindState::Thinking => 15,
            MindState::Considering => 20,
            MindState::Pondering => 25,
            MindState::Ruminating => 30,
            MindState::Focusing => 35,
            MindState::Deliberating => 40,
            MindState::Concentrating => 45,
            MindState::Attentive => 50,
            MindState::Distracted => 55,
            MindState::Muddled => 60,
            MindState::BecomingFuzzy => 65,
            MindState::Fuzzy => 70,
            MindState::SlightlyDizzy => 75,
            MindState::Dizzy => 80,
            MindState::VeryDizzy => 85,
            MindState::Ropy => 90,
            MindState::Stunned => 100,
        }
    }
}

impl EncumbranceState {
    pub fn to_str(&self) -> &'static str {
        match self {
            EncumbranceState::None => "none",
            EncumbranceState::Light => "light",
            EncumbranceState::Moderate => "moderate",
            EncumbranceState::Heavy => "heavy",
            EncumbranceState::VeryHeavy => "very heavy",
            EncumbranceState::Overburdened => "overburdened",
        }
    }
    pub fn to_value(&self) -> i64 {
        match self {
            EncumbranceState::None => 0,
            EncumbranceState::Light => 1,
            EncumbranceState::Moderate => 2,
            EncumbranceState::Heavy => 3,
            EncumbranceState::VeryHeavy => 4,
            EncumbranceState::Overburdened => 5,
        }
    }
}

impl Game {
    pub fn to_str(&self) -> &'static str {
        match self {
            Game::GemStone => "GS",
            Game::DragonRealms => "DR",
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct ActiveSpell {
    pub name: String,
    pub duration_secs: Option<u32>,
}

// Note: GameState intentionally omits PartialEq — Instant does not implement PartialEq
#[derive(Debug, Clone, Default)]
pub struct GameState {
    pub health: u32, pub max_health: u32,
    pub mana: u32,   pub max_mana: u32,
    pub spirit: u32, pub max_spirit: u32,
    pub stamina: u32, pub max_stamina: u32,
    pub concentration: u32, pub max_concentration: u32,

    pub roundtime_end: Option<Instant>,
    pub cast_roundtime_end: Option<Instant>,

    pub room_name: String,
    pub room_description: String,
    pub room_exits: Vec<String>,
    pub room_id: Option<u32>,

    pub prepared_spell: Option<String>,
    pub active_spells: Vec<ActiveSpell>,

    pub stance: Stance,
    pub mind: MindState,
    pub encumbrance: EncumbranceState,

    // Status indicators from <indicator id="IconXXX" visible="y/n"/>
    pub bleeding: bool,
    pub stunned: bool,
    pub dead: bool,
    pub sleeping: bool,
    pub prone: bool,
    pub sitting: bool,
    pub kneeling: bool,

    pub right_hand: Option<String>,
    pub left_hand: Option<String>,

    pub server_time: i64,
    pub prompt: String,
    pub level: u32,
    pub experience: u64,
    pub game: Game,

    pub name: String,
    pub room_count: u32,
}

impl GameState {
    /// Seconds of roundtime remaining (0.0 if none).
    pub fn roundtime(&self) -> f64 {
        match self.roundtime_end {
            Some(end) => {
                let now = Instant::now();
                if end > now { (end - now).as_secs_f64() } else { 0.0 }
            }
            None => 0.0,
        }
    }

    /// Seconds of cast roundtime remaining (0.0 if none).
    pub fn cast_roundtime(&self) -> f64 {
        match self.cast_roundtime_end {
            Some(end) => {
                let now = Instant::now();
                if end > now { (end - now).as_secs_f64() } else { 0.0 }
            }
            None => 0.0,
        }
    }

    pub fn apply(&mut self, event: XmlEvent) {
        match event {
            XmlEvent::Health { value, max }        => {
                tracing::info!("GameState health: {value}/{max:?}");
                self.health = value;
                if let Some(m) = max { self.max_health = m; }
            }
            XmlEvent::Mana { value, max }          => { self.mana = value;          if let Some(m) = max { self.max_mana = m; } }
            XmlEvent::Spirit { value, max }        => { self.spirit = value;        if let Some(m) = max { self.max_spirit = m; } }
            XmlEvent::Stamina { value, max }       => { self.stamina = value;       if let Some(m) = max { self.max_stamina = m; } }
            XmlEvent::Concentration { value, max } => { self.concentration = value; if let Some(m) = max { self.max_concentration = m; } }
            XmlEvent::RoundTime { end_epoch }      => self.roundtime_end = epoch_to_instant(end_epoch),
            XmlEvent::CastTime { end_epoch }       => self.cast_roundtime_end = epoch_to_instant(end_epoch),
            XmlEvent::Prompt { time, text }        => { self.server_time = time; self.prompt = text; }
            XmlEvent::RoomName { name }            => self.room_name = name,
            XmlEvent::RoomDescription { text }     => self.room_description = text,
            XmlEvent::RoomExits { exits }          => self.room_exits = exits,
            XmlEvent::RoomId { id }                => { self.room_id = Some(id); self.room_count += 1; },
            XmlEvent::PreparedSpell { name }       => self.prepared_spell = Some(name),
            XmlEvent::SpellCleared                 => self.prepared_spell = None,
            XmlEvent::Level { value }              => self.level = value,
            XmlEvent::RightHand { item }           => self.right_hand = item,
            XmlEvent::LeftHand { item }            => self.left_hand = item,
            XmlEvent::Mode { room_id, .. }         => { if let Some(id) = room_id { self.room_id = Some(id); } }
            XmlEvent::Indicator { name, visible }  => match name.as_str() {
                "IconBLEEDING" => self.bleeding = visible,
                "IconSTUNNED"  => self.stunned = visible,
                "IconDEAD"     => self.dead = visible,
                "IconSLEEPING" => self.sleeping = visible,
                "IconPRONE"    => self.prone = visible,
                "IconSITTING"  => self.sitting = visible,
                "IconKNEELING" => self.kneeling = visible,
                _ => {}
            },
            XmlEvent::ActiveSpell { name, duration } => {
                self.active_spells.push(ActiveSpell {
                    name,
                    duration_secs: duration,
                });
            }
            _ => {}
        }
    }
}

/// Convert a Unix epoch i64 to an Instant offset from now.
/// Returns None if the epoch is in the past.
/// Note: mixes SystemTime (for epoch math) and Instant (monotonic). NTP adjustments
/// can cause small drift; acceptable for v1 roundtime display.
fn epoch_to_instant(epoch: i64) -> Option<Instant> {
    use std::time::{SystemTime, UNIX_EPOCH};
    let now_epoch = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()?
        .as_secs() as i64;
    let delta = epoch - now_epoch;
    if delta <= 0 { None } else { Some(Instant::now() + Duration::from_secs(delta as u64)) }
}
