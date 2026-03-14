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
    pub fn as_str(&self) -> Option<&'static str> {
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
    pub fn as_str(&self) -> &'static str {
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
    pub fn as_str(&self) -> &'static str {
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
    pub fn as_str(&self) -> &'static str {
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

/// Wound or scar severity for each of 16 body parts (0 = none, 1-3 = severity).
#[derive(Debug, Clone, Default)]
pub struct BodyInjuries {
    pub head: u8,
    pub neck: u8,
    pub back: u8,
    pub chest: u8,
    pub abdomen: u8,
    pub left_eye: u8,
    pub right_eye: u8,
    pub left_arm: u8,
    pub right_arm: u8,
    pub left_hand: u8,
    pub right_hand: u8,
    pub left_leg: u8,
    pub right_leg: u8,
    pub left_foot: u8,
    pub right_foot: u8,
    pub nsys: u8,
}

impl BodyInjuries {
    /// Set the severity for a body part by its XML id string.
    /// Returns false if the id is not recognized.
    pub fn set(&mut self, xml_id: &str, severity: u8) -> bool {
        match xml_id {
            "head"      => self.head = severity,
            "neck"      => self.neck = severity,
            "back"      => self.back = severity,
            "chest"     => self.chest = severity,
            "abdomen"   => self.abdomen = severity,
            "leftEye"   => self.left_eye = severity,
            "rightEye"  => self.right_eye = severity,
            "leftArm"   => self.left_arm = severity,
            "rightArm"  => self.right_arm = severity,
            "leftHand"  => self.left_hand = severity,
            "rightHand" => self.right_hand = severity,
            "leftLeg"   => self.left_leg = severity,
            "rightLeg"  => self.right_leg = severity,
            "leftFoot"  => self.left_foot = severity,
            "rightFoot" => self.right_foot = severity,
            "nsys"      => self.nsys = severity,
            _ => return false,
        }
        true
    }

    /// Get severity by Lua key (snake_case or camelCase).
    pub fn get(&self, key: &str) -> Option<u8> {
        match key {
            "head"                          => Some(self.head),
            "neck"                          => Some(self.neck),
            "back"                          => Some(self.back),
            "chest"                         => Some(self.chest),
            "abdomen" | "abs"               => Some(self.abdomen),
            "left_eye"  | "leftEye"  | "leye"  => Some(self.left_eye),
            "right_eye" | "rightEye" | "reye"  => Some(self.right_eye),
            "left_arm"  | "leftArm"  | "larm"  => Some(self.left_arm),
            "right_arm" | "rightArm" | "rarm"  => Some(self.right_arm),
            "left_hand" | "leftHand" | "lhand" => Some(self.left_hand),
            "right_hand"| "rightHand"| "rhand" => Some(self.right_hand),
            "left_leg"  | "leftLeg"  | "lleg"  => Some(self.left_leg),
            "right_leg" | "rightLeg" | "rleg"  => Some(self.right_leg),
            "left_foot" | "leftFoot" | "lfoot" => Some(self.left_foot),
            "right_foot"| "rightFoot"| "rfoot" => Some(self.right_foot),
            "nsys" | "nerves"                   => Some(self.nsys),
            _ => None,
        }
    }
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
    // Additional status indicators (Lich5 parity)
    pub standing: bool,
    pub poisoned: bool,
    pub diseased: bool,
    pub hidden: bool,
    pub invisible: bool,
    pub webbed: bool,
    pub joined: bool,
    pub calmed: bool,
    pub cutthroat: bool,
    pub silenced: bool,
    pub bound: bool,

    pub right_hand: Option<String>,
    pub left_hand: Option<String>,

    pub wounds: BodyInjuries,
    pub scars: BodyInjuries,

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
            XmlEvent::Experience { value }         => self.experience = value,
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
                "IconKNEELING"  => self.kneeling = visible,
                "IconSTANDING"  => self.standing = visible,
                "IconPOISONED"  => self.poisoned = visible,
                "IconDISEASED"  => self.diseased = visible,
                "IconHIDDEN"    => self.hidden = visible,
                "IconINVISIBLE" => self.invisible = visible,
                "IconWEBBED"    => self.webbed = visible,
                "IconJOINED"    => self.joined = visible,
                // calmed, cutthroat, silenced, bound: Icon names not yet confirmed from GS protocol
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
