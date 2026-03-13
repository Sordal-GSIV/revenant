use std::time::Instant;

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
}
