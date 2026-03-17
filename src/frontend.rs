use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Frontend {
    Wrayth,
    Stormfront,
    Wizard,
    Avalon,
    Genie,
    Frostbite,
    Profanity,
    Mudlet,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Capability {
    Xml,
    Gsl,
    Streams,
    Mono,
    RoomWindow,
}

impl Frontend {
    pub fn capabilities(&self) -> &'static [Capability] {
        use Capability::*;
        match self {
            Self::Wrayth     => &[Xml, Streams, Mono, RoomWindow],
            Self::Stormfront => &[Xml, Streams, Mono, RoomWindow],
            Self::Wizard     => &[Gsl],
            Self::Avalon     => &[Gsl],
            Self::Genie      => &[Xml, Mono],
            Self::Frostbite  => &[Xml],
            Self::Profanity  => &[Xml, Streams],
            Self::Mudlet     => &[Xml],
            Self::Unknown    => &[],
        }
    }

    pub fn supports(&self, cap: Capability) -> bool {
        self.capabilities().contains(&cap)
    }

    pub fn from_name(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "wrayth" | "stormfront" => Self::Wrayth,
            "wizard" => Self::Wizard,
            "avalon" => Self::Avalon,
            "genie" => Self::Genie,
            "frostbite" => Self::Frostbite,
            "profanity" => Self::Profanity,
            "mudlet" => Self::Mudlet,
            _ => Self::Unknown,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Wrayth => "wrayth",
            Self::Stormfront => "stormfront",
            Self::Wizard => "wizard",
            Self::Avalon => "avalon",
            Self::Genie => "genie",
            Self::Frostbite => "frostbite",
            Self::Profanity => "profanity",
            Self::Mudlet => "mudlet",
            Self::Unknown => "unknown",
        }
    }
}

impl fmt::Display for Frontend {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl Default for Frontend {
    fn default() -> Self { Self::Wrayth }
}
