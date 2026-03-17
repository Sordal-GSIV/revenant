//! StormFront XML → GSL (Game Script Language) converter for Wizard/Avalon frontends.
//!
//! These older frontends don't understand the XML protocol; they need GSL binary
//! marker codes instead.  This module is a faithful port of Lich5's `sf_to_wiz`
//! function from `lib/global_defs.rb`.

use regex::Regex;
use std::sync::OnceLock;

/// GSL escape character (0x1C).
const ESC: &str = "\x1c";

/// Stateful converter that transforms XML output lines into GSL format.
pub struct GslConverter {
    /// When we receive more pushStream opens than popStream closes in a single
    /// line, we buffer the partial line here and wait for the rest.
    multiline_buf: Option<String>,
}

impl GslConverter {
    pub fn new() -> Self {
        Self {
            multiline_buf: None,
        }
    }

    /// Convert an XML string to GSL format.
    ///
    /// Returns `None` when the output should be completely suppressed (empty
    /// after conversion, or buffering a multiline fragment).
    pub fn convert(&mut self, xml_line: &str) -> Option<String> {
        // Pass through bare newlines
        if xml_line == "\r\n" || xml_line == "\n" {
            return Some(xml_line.to_string());
        }

        // --- Multiline buffering ---
        // If we have a buffered partial, prepend it.
        let mut line = if let Some(buf) = self.multiline_buf.take() {
            buf + xml_line
        } else {
            xml_line.to_string()
        };

        // If pushStream opens exceed popStream closes, buffer and wait.
        let push_count = count_occurrences(&line, r"<pushStream[^>]*/>");
        let pop_count = count_occurrences(&line, r"<popStream[^>]*/>");
        if push_count > pop_count {
            self.multiline_buf = Some(line);
            return None;
        }

        // If style-open exceeds style-reset, buffer and wait.
        let style_open = count_occurrences(&line, r#"<style id="\w+"[^>]*/>"#);
        let style_reset = count_occurrences(&line, r#"<style id=""[^>]*/>"#);
        if style_open > style_reset {
            self.multiline_buf = Some(line);
            return None;
        }

        // --- LaunchURL ---
        if let Some(caps) = re_launch_url().captures(&line) {
            let url = &caps[1];
            // Lich sends this as a separate write; we inline it.
            let launch = format!("{ESC}GSw00005\r\nhttps://www.play.net{url}\r\n");
            line = re_launch_url().replace(&line, &launch).to_string();
        }

        // --- Speech preset ---
        // <preset id='speech'>text</preset>  →  just the text (no special GSL code)
        line = re_speech_preset()
            .replace_all(&line, "$1")
            .to_string();

        // --- Thought streams ---
        // <pushStream id="thoughts"...>[Channel] msg<popStream/>
        line = re_thoughts_push()
            .replace_all(&line, |caps: &regex::Captures| {
                let channel = caps[1].replace(' ', "-");
                let mut msg = caps[2].to_string();
                msg = msg.replace("<pushBold/>", "");
                msg = msg.replace("<popBold/>", "");
                format!("You hear the faint thoughts of [{channel}]-ESP echo in your mind:\r\n{msg}")
            })
            .to_string();

        // --- Voln thoughts ---
        line = re_voln_push()
            .replace_all(&line, |caps: &regex::Captures| {
                let name = &caps[1];
                let quote = &caps[2];
                format!("The Symbol of Thought begins to burn in your mind and you hear {name} thinking, {quote}\r\n")
            })
            .to_string();

        // --- <stream id="thoughts"> (alternate format) ---
        line = re_thoughts_stream()
            .replace_all(&line, |caps: &regex::Captures| {
                let who = &caps[1];
                let msg = &caps[2];
                format!("You hear the faint thoughts of {who} echo in your mind:\r\n{msg}")
            })
            .to_string();

        // --- Familiar stream ---
        // <pushStream id="familiar"...>content<popStream/>  →  ESC GSe \r\n content ESC GSf \r\n
        line = re_familiar_push()
            .replace_all(&line, |caps: &regex::Captures| {
                let content = &caps[1];
                format!("{ESC}GSe\r\n{content}{ESC}GSf\r\n")
            })
            .to_string();

        // --- Death stream ---
        line = re_death_push()
            .replace_all(&line, |caps: &regex::Captures| {
                let content = &caps[1];
                format!("{ESC}GSw00003\r\n{content}{ESC}GSw00004\r\n")
            })
            .to_string();

        // --- Room name style ---
        // <style id="roomName" />text<style id=""/>  →  ESC GSo \r\n text ESC GSp \r\n
        line = re_room_name_style()
            .replace_all(&line, |caps: &regex::Captures| {
                let text = &caps[1];
                format!("{ESC}GSo\r\n{text}{ESC}GSp\r\n")
            })
            .to_string();

        // --- Empty room desc (no content between tags) → strip entirely ---
        line = re_room_desc_empty().replace_all(&line, "").to_string();

        // --- Room desc style ---
        // <style id="roomDesc"/>text<style id=""/>  →  ESC GSH \r\n text ESC GSI \r\n
        // Links inside roomDesc get highlight markers.
        line = re_room_desc_style()
            .replace_all(&line, |caps: &regex::Captures| {
                let mut desc = caps[1].to_string();
                desc = re_link_open().replace_all(&desc, |_: &regex::Captures| {
                    format!("{ESC}GSA")
                }).to_string();
                desc = desc.replace("</a>", &format!("{ESC}GSa"));
                format!("{ESC}GSH\r\n{desc}{ESC}GSI\r\n")
            })
            .to_string();

        // --- Prompt: strip trailing \r\n after </prompt> ---
        line = line.replace("</prompt>\r\n", "</prompt>");
        line = line.replace("</prompt>\n", "</prompt>");

        // --- Bold ---
        line = line.replace("<pushBold/>", &format!("{ESC}GSL\r\n"));
        line = line.replace("<popBold/>", &format!("{ESC}GSM\r\n"));

        // --- Suppress known stream types ---
        // spellfront, inv, bounty, society, speech, talk
        line = re_suppressed_streams()
            .replace_all(&line, "")
            .to_string();

        // --- Suppress <stream id="Spells">...</stream> ---
        line = re_spells_stream()
            .replace_all(&line, "")
            .to_string();

        // --- Strip compDef, inv, component, right, left, spell, prompt with content ---
        line = re_strip_content_tags()
            .replace_all(&line, "")
            .to_string();

        // --- Strip all remaining XML tags ---
        line = re_strip_all_tags()
            .replace_all(&line, "")
            .to_string();

        // --- Decode XML entities ---
        line = line.replace("&gt;", ">");
        line = line.replace("&lt;", "<");
        line = line.replace("&amp;", "&");

        // --- Suppress if empty after stripping ---
        if line.replace("\r\n", "").replace('\n', "").is_empty() {
            return None;
        }

        Some(line)
    }
}

// ---------------------------------------------------------------------------
// Regex helpers — each compiled once via OnceLock
// ---------------------------------------------------------------------------

fn count_occurrences(s: &str, pattern: &str) -> usize {
    let re = Regex::new(pattern).unwrap();
    re.find_iter(s).count()
}

macro_rules! cached_re {
    ($name:ident, $pat:expr) => {
        fn $name() -> &'static Regex {
            static RE: OnceLock<Regex> = OnceLock::new();
            RE.get_or_init(|| Regex::new($pat).unwrap())
        }
    };
}

cached_re!(re_launch_url, r#"<LaunchURL src="(.*?)" />"#);
cached_re!(re_speech_preset, r#"(?s)<preset id='speech'>(.*?)</preset>"#);
cached_re!(re_thoughts_push, r#"(?s)<pushStream id="thoughts"[^>]*>\[([^\]]+?)\]\s*(.*?)<popStream/>"#);
cached_re!(re_voln_push, r#"(?s)<pushStream id="voln"[^>]*>\[Voln - (?:<a[^>]*>)?([A-Z][a-z]+)(?:</a>)?\]\s*(".*?")[\r\n]*<popStream/>"#);
cached_re!(re_thoughts_stream, r#"(?s)<stream id="thoughts"[^>]*>([^:]+): (.*?)</stream>"#);
cached_re!(re_familiar_push, r#"(?s)<pushStream id="familiar"[^>]*>(.*)<popStream/>"#);
cached_re!(re_death_push, r#"(?s)<pushStream id="death"/>(.*?)<popStream/>"#);
cached_re!(re_room_name_style, r#"(?s)<style id="roomName" />(.*?)<style id=""/>"#);
cached_re!(re_room_desc_empty, r#"<style id="roomDesc"/><style id=""/>\r?\n"#);
cached_re!(re_room_desc_style, r#"(?s)<style id="roomDesc"/>(.*?)<style id=""/>"#);
cached_re!(re_link_open, r#"<a[^>]*>"#);
cached_re!(re_suppressed_streams, r#"(?s)<pushStream id=["'](?:spellfront|inv|bounty|society|speech|talk)["'][^>]*/>(.*?)<popStream[^>]*/>"#);
cached_re!(re_spells_stream, r#"(?s)<stream id="Spells">.*?</stream>"#);
cached_re!(re_strip_content_tags, r#"(?s)(?:<compDef[^>]*>.*?</compDef>|<inv[^>]*>.*?</inv>|<component[^>]*>.*?</component>|<right[^>]*>.*?</right>|<left[^>]*>.*?</left>|<spell[^>]*>.*?</spell>|<prompt[^>]*>.*?</prompt>)"#);
cached_re!(re_strip_all_tags, r"<[^>]+>");

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bare_newline_passthrough() {
        let mut c = GslConverter::new();
        assert_eq!(c.convert("\r\n"), Some("\r\n".to_string()));
        assert_eq!(c.convert("\n"), Some("\n".to_string()));
    }

    #[test]
    fn test_room_name_conversion() {
        let mut c = GslConverter::new();
        let input = r#"<style id="roomName" />Town Square<style id=""/>"#;
        let result = c.convert(input).unwrap();
        assert!(result.contains("\x1cGSo\r\nTown Square\x1cGSp\r\n"));
    }

    #[test]
    fn test_room_desc_conversion() {
        let mut c = GslConverter::new();
        let input = r#"<style id="roomDesc"/>A dusty road leads north.<style id=""/>"#;
        let result = c.convert(input).unwrap();
        assert!(result.contains("\x1cGSH\r\nA dusty road leads north.\x1cGSI\r\n"));
    }

    #[test]
    fn test_room_desc_with_links() {
        let mut c = GslConverter::new();
        let input = r#"<style id="roomDesc"/>You see <a exist="123" noun="chest">a chest</a> here.<style id=""/>"#;
        let result = c.convert(input).unwrap();
        assert!(result.contains("\x1cGSA"));
        assert!(result.contains("a chest\x1cGSa"));
    }

    #[test]
    fn test_empty_room_desc_stripped() {
        let mut c = GslConverter::new();
        let input = "<style id=\"roomDesc\"/><style id=\"\"/>\r\nSome text\n";
        let result = c.convert(input).unwrap();
        assert!(!result.contains("roomDesc"));
        assert!(result.contains("Some text"));
    }

    #[test]
    fn test_bold_conversion() {
        let mut c = GslConverter::new();
        let input = "<pushBold/>a goblin<popBold/>";
        let result = c.convert(input).unwrap();
        assert_eq!(result, "\x1cGSL\r\na goblin\x1cGSM\r\n");
    }

    #[test]
    fn test_thoughts_conversion() {
        let mut c = GslConverter::new();
        let input = r#"<pushStream id="thoughts"/>[General] Hello world<popStream/>"#;
        let result = c.convert(input).unwrap();
        assert!(result.contains("You hear the faint thoughts of [General]-ESP echo in your mind:"));
        assert!(result.contains("Hello world"));
    }

    #[test]
    fn test_familiar_conversion() {
        let mut c = GslConverter::new();
        let input = r#"<pushStream id="familiar"/>Your familiar sees something.<popStream/>"#;
        let result = c.convert(input).unwrap();
        assert!(result.contains("\x1cGSe\r\n"));
        assert!(result.contains("Your familiar sees something."));
        assert!(result.contains("\x1cGSf\r\n"));
    }

    #[test]
    fn test_death_conversion() {
        let mut c = GslConverter::new();
        let input = r#"<pushStream id="death"/>You have died.<popStream/>"#;
        let result = c.convert(input).unwrap();
        assert!(result.contains("\x1cGSw00003\r\n"));
        assert!(result.contains("You have died."));
        assert!(result.contains("\x1cGSw00004\r\n"));
    }

    #[test]
    fn test_suppressed_streams() {
        let mut c = GslConverter::new();
        let input = r#"<pushStream id="inv"/>some inventory data<popStream/>"#;
        // After stripping the stream the line is empty → None
        assert_eq!(c.convert(input), None);
    }

    #[test]
    fn test_spells_stream_stripped() {
        let mut c = GslConverter::new();
        let input = r#"<stream id="Spells">101 Spirit Warding I</stream>"#;
        assert_eq!(c.convert(input), None);
    }

    #[test]
    fn test_content_tags_stripped() {
        let mut c = GslConverter::new();
        let input = r#"<compDef id="room objs">stuff</compDef>Hello"#;
        let result = c.convert(input).unwrap();
        assert!(!result.contains("compDef"));
        assert!(!result.contains("stuff"));
        assert!(result.contains("Hello"));
    }

    #[test]
    fn test_prompt_stripped() {
        let mut c = GslConverter::new();
        // Prompt tag + content is fully stripped by the content-tag regex
        let input = "<prompt time=\"12345\">&gt;</prompt>More text\n";
        let result = c.convert(input).unwrap();
        assert!(!result.contains("prompt"));
        assert!(result.contains("More text"));
    }

    #[test]
    fn test_xml_entity_decoding() {
        let mut c = GslConverter::new();
        let input = "You say &gt;&lt;&amp; done\n";
        let result = c.convert(input).unwrap();
        assert!(result.contains("><& done"));
    }

    #[test]
    fn test_all_tags_stripped() {
        let mut c = GslConverter::new();
        let input = r#"<d cmd="look">look</d> around"#;
        let result = c.convert(input).unwrap();
        assert_eq!(result, "look around");
    }

    #[test]
    fn test_multiline_buffering() {
        let mut c = GslConverter::new();
        // pushStream without matching popStream → buffer
        let part1 = r#"<pushStream id="thoughts"/>[General] Hello "#;
        assert_eq!(c.convert(part1), None);

        // Now complete it
        let part2 = "world<popStream/>\n";
        let result = c.convert(part2).unwrap();
        assert!(result.contains("You hear the faint thoughts of [General]-ESP echo in your mind:"));
        assert!(result.contains("Hello world"));
    }

    #[test]
    fn test_launch_url() {
        let mut c = GslConverter::new();
        let input = r#"<LaunchURL src="/play/gemstone" />"#;
        let result = c.convert(input).unwrap();
        assert!(result.contains("\x1cGSw00005\r\nhttps://www.play.net/play/gemstone\r\n"));
    }

    #[test]
    fn test_speech_preset() {
        let mut c = GslConverter::new();
        let input = r#"<preset id='speech'>You say, "Hello."</preset>"#;
        let result = c.convert(input).unwrap();
        assert!(result.contains("You say, \"Hello.\""));
        assert!(!result.contains("preset"));
    }

    #[test]
    fn test_empty_after_strip_returns_none() {
        let mut c = GslConverter::new();
        let input = r#"<progressBar id="health" value="100"/>"#;
        assert_eq!(c.convert(input), None);
    }

    #[test]
    fn test_plain_text_passthrough() {
        let mut c = GslConverter::new();
        let input = "You also see a wooden chest.\r\n";
        assert_eq!(c.convert(input), Some(input.to_string()));
    }
}
