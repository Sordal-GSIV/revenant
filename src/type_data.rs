use std::collections::HashMap;
use anyhow::Result;
use quick_xml::events::Event;
use quick_xml::Reader;

/// GameObj type/sellable classification data loaded from gameobj-data.xml.
pub struct TypeData {
    noun_types: HashMap<String, String>,
    name_types: HashMap<String, String>,
    noun_sellable: HashMap<String, String>,
    name_sellable: HashMap<String, String>,
}

impl TypeData {
    pub fn load(path: &str) -> Result<Self> {
        let content = std::fs::read_to_string(path)?;
        Self::parse(&content)
    }

    pub fn parse(xml: &str) -> Result<Self> {
        let mut noun_types: HashMap<String, String> = HashMap::new();
        let mut name_types: HashMap<String, String> = HashMap::new();
        let mut noun_sellable: HashMap<String, String> = HashMap::new();
        let mut name_sellable: HashMap<String, String> = HashMap::new();

        let mut reader = Reader::from_str(xml);
        reader.config_mut().trim_text(true);

        enum Context {
            None,
            Type { type_name: String },
            Sellable { sellable_name: String },
        }
        let mut ctx = Context::None;
        let mut current_element: Option<String> = None;

        loop {
            match reader.read_event() {
                Ok(Event::Start(ref e)) => {
                    let tag = std::str::from_utf8(e.name().as_ref())?.to_string();
                    match tag.as_str() {
                        "type" => {
                            let name = e.attributes().flatten()
                                .find(|a| std::str::from_utf8(a.key.as_ref()).ok() == Some("name"))
                                .and_then(|a| a.unescape_value().ok())
                                .map(|v| v.into_owned())
                                .unwrap_or_default();
                            ctx = Context::Type { type_name: name };
                        }
                        "sellable" => {
                            let name = e.attributes().flatten()
                                .find(|a| std::str::from_utf8(a.key.as_ref()).ok() == Some("name"))
                                .and_then(|a| a.unescape_value().ok())
                                .map(|v| v.into_owned())
                                .unwrap_or_default();
                            ctx = Context::Sellable { sellable_name: name };
                        }
                        "noun" | "name" => {
                            current_element = Some(tag);
                        }
                        _ => {}
                    }
                }
                Ok(Event::Text(ref e)) => {
                    if let Some(ref elem) = current_element {
                        let text = e.decode()?.into_owned().to_lowercase();
                        match (&ctx, elem.as_str()) {
                            (Context::Type { type_name }, "noun") => {
                                noun_types.insert(text, type_name.clone());
                            }
                            (Context::Type { type_name }, "name") => {
                                name_types.insert(text, type_name.clone());
                            }
                            (Context::Sellable { sellable_name }, "noun") => {
                                noun_sellable.insert(text, sellable_name.clone());
                            }
                            (Context::Sellable { sellable_name }, "name") => {
                                name_sellable.insert(text, sellable_name.clone());
                            }
                            _ => {}
                        }
                    }
                }
                Ok(Event::End(ref e)) => {
                    let name_bytes = e.name();
                    let tag = std::str::from_utf8(name_bytes.as_ref())?;
                    match tag {
                        "type" | "sellable" => ctx = Context::None,
                        "noun" | "name" => current_element = None,
                        _ => {}
                    }
                }
                Ok(Event::Eof) => break,
                Err(e) => {
                    tracing::warn!("gameobj-data.xml parse error: {e}");
                    break;
                }
                _ => {}
            }
        }

        Ok(Self { noun_types, name_types, noun_sellable, name_sellable })
    }

    /// Get comma-separated type tags for a game object by noun and name.
    /// Checks noun first, then full name (matching Lich5 lookup order).
    pub fn get_type(&self, noun: &str, name: &str) -> Option<&str> {
        let noun_lower = noun.to_lowercase();
        let name_lower = name.to_lowercase();
        self.noun_types.get(&noun_lower)
            .or_else(|| self.name_types.get(&name_lower))
            .map(|s| s.as_str())
    }

    /// Check if a game object matches a specific type tag.
    pub fn is_type(&self, noun: &str, name: &str, type_tag: &str) -> bool {
        match self.get_type(noun, name) {
            Some(types) => types.split(',').any(|t| t.trim() == type_tag),
            None => false,
        }
    }

    /// Get the sellable category for a game object.
    pub fn get_sellable(&self, noun: &str, name: &str) -> Option<&str> {
        let noun_lower = noun.to_lowercase();
        let name_lower = name.to_lowercase();
        self.noun_sellable.get(&noun_lower)
            .or_else(|| self.name_sellable.get(&name_lower))
            .map(|s| s.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_type_data_parse() {
        let xml = r#"<?xml version="1.0"?>
<data>
  <type name="gem">
    <noun>ruby</noun>
    <noun>emerald</noun>
    <name>star ruby</name>
  </type>
  <type name="herb,forageable">
    <noun>acantha</noun>
  </type>
  <sellable name="gemshop">
    <noun>ruby</noun>
  </sellable>
</data>"#;

        let td = TypeData::parse(xml).unwrap();
        assert_eq!(td.get_type("ruby", "a shimmering ruby"), Some("gem"));
        assert_eq!(td.get_type("emerald", "a green emerald"), Some("gem"));
        assert_eq!(td.get_type("acantha", "some acantha leaf"), Some("herb,forageable"));
        assert!(td.is_type("ruby", "", "gem"));
        assert!(td.is_type("acantha", "", "herb"));
        assert!(td.is_type("acantha", "", "forageable"));
        assert!(!td.is_type("ruby", "", "herb"));
        assert_eq!(td.get_sellable("ruby", ""), Some("gemshop"));
        assert_eq!(td.get_sellable("sword", ""), None);

        // Name-based type lookup
        assert_eq!(td.get_type("xxx", "star ruby"), Some("gem"));
    }
}
