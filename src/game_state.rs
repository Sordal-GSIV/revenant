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
            XmlEvent::Health { value, max }        => { self.health = value;        if let Some(m) = max { self.max_health = m; } }
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
            XmlEvent::RoomId { id }                => self.room_id = Some(id),
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
