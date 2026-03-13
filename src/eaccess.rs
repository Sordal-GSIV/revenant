// SGE eAccess authentication module.
//
// Reverse-engineered from lich-5/lib/common/authentication/eaccess.rb.
//
// Key findings from the reference implementation:
// - Host: eaccess.play.net, Port: 7910 (confirmed)
// - Transport: SSL/TLS over TCP (OpenSSL in Ruby; tokio-rustls here)
// - Hash algorithm: for each byte b at index i:
//     result[i] = ((b - 32) ^ hashkey[i % key_len]) + 32
//   The Ruby source uses index-for-index with no explicit modulo, but the
//   server key length matches the password length in practice. We use modulo
//   wrapping as a safety net.
// - Protocol sequence (non-legacy path):
//     K  → receive hash key
//     A  → authenticate (send account + hashed password)
//     M  → get game list (response must start with "M\t")
//     F  → select game (response must match NORMAL|PREMIUM|TRIAL|INTERNAL|FREE)
//     G  → (required, response ignored) — MISSING from plan, present in eaccess.rb
//     P  → (required, response ignored) — MISSING from plan, present in eaccess.rb
//     C  → get character list
//     L  → login as character
// - C response: strip header matching /^C\t\d+\t\d+\t\d+\t\d+[\t\n]/ then
//   scan ID\tName pairs
// - L response: strip "L\tOK\t" prefix, then split on "\t", parse "k=v" pairs
//   with LOWERCASE keys: "gamehost", "gameport", "key"
// - Auth validation: response must match /KEY\t/ (not just absence of "PROBLEM")
// - Read: sysread(8192) — single packet read, not line-by-line

use anyhow::{bail, Context, Result};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio_native_tls::TlsConnector;
use tracing::debug;

/// eAccess server hostname.
pub const SGE_HOST: &str = "eaccess.play.net";

/// eAccess server port — confirmed 7910 from eaccess.rb.
pub const SGE_PORT: u16 = 7910;

/// Maximum packet size — matches PACKET_SIZE = 8192 in eaccess.rb.
const PACKET_SIZE: usize = 8192;

#[derive(Debug, Clone)]
pub struct Session {
    pub host: String,
    pub port: u16,
    pub key: String,
    pub game: String,
    pub character: String,
    /// All raw key=value pairs from the L response, used for Avalon SAL file generation.
    pub raw_fields: Vec<(String, String)>,
}

/// Hash the SGE password using the server-provided key.
///
/// Algorithm (from eaccess.rb line 93):
///   `password.each_index { |i| password[i] = ((password[i] - 32) ^ hashkey[i]) + 32 }`
///
/// Each output byte = `((input_byte - 32) ^ key_byte[i % key_len]) + 32`.
/// The modulo is a safety net; in practice the server key length matches.
/// Returns raw bytes as a String (may contain non-UTF8; lossy conversion matches Ruby behavior).
pub fn hash_password(password: &str, key: &str) -> String {
    let key_bytes: Vec<u8> = key.trim().bytes().collect();
    if key_bytes.is_empty() {
        return password.to_string();
    }
    let result: Vec<u8> = password
        .bytes()
        .enumerate()
        .map(|(i, b)| {
            let k = key_bytes[i % key_bytes.len()];
            (b.wrapping_sub(32) ^ k).wrapping_add(32)
        })
        .collect();
    String::from_utf8_lossy(&result).into_owned()
}

/// Build a TLS connector that accepts any server certificate.
///
/// Uses native-tls (OpenSSL on Linux, SChannel on Windows, SecureTransport on macOS)
/// for maximum cipher-suite compatibility with the SGE eAccess server, matching
/// the Ruby/OpenSSL transport used by the reference implementation.
fn build_tls_connector() -> Result<TlsConnector> {
    let native = native_tls::TlsConnector::builder()
        .danger_accept_invalid_certs(true)
        .danger_accept_invalid_hostnames(true)
        .build()?;
    Ok(TlsConnector::from(native))
}

/// Read a single packet from the TLS stream (up to PACKET_SIZE bytes).
///
/// Matches `EAccess.read(conn)` → `conn.sysread(PACKET_SIZE)` in eaccess.rb.
/// Returns the packet as a trimmed String.
async fn read_packet<S>(stream: &mut S) -> Result<String>
where
    S: AsyncReadExt + Unpin,
{
    let mut buf = vec![0u8; PACKET_SIZE];
    let n = stream.read(&mut buf).await?;
    if n == 0 {
        bail!("SGE connection closed unexpectedly");
    }
    Ok(String::from_utf8_lossy(&buf[..n]).trim().to_string())
}

#[derive(Debug, Clone, PartialEq)]
pub struct CharacterEntry {
    pub id: String,
    pub name: String,
}

/// Connect to eAccess and return the character list for `game_code`.
/// Runs K→A→M→F→G→P→C. Does NOT send L (login).
pub async fn list_characters(
    account: &str,
    password: &str,
    game_code: &str,
) -> Result<Vec<CharacterEntry>> {
    let connector = build_tls_connector()?;
    let tcp = TcpStream::connect((SGE_HOST, SGE_PORT)).await?;
    let mut stream = connector.connect(SGE_HOST, tcp).await?;

    stream.write_all(b"K\n").await?;
    let key = read_packet(&mut stream).await?;
    let hash = hash_password(password, &key);
    stream.write_all(format!("A\t{account}\t{hash}\n").as_bytes()).await?;
    let auth_resp = read_packet(&mut stream).await?;
    if !auth_resp.contains("KEY\t") {
        bail!("SGE auth failed: {auth_resp}");
    }
    stream.write_all(b"M\n").await?;
    let _ = read_packet(&mut stream).await?;
    stream.write_all(format!("F\t{game_code}\n").as_bytes()).await?;
    let _ = read_packet(&mut stream).await?;
    stream.write_all(format!("G\t{game_code}\n").as_bytes()).await?;
    let _ = read_packet(&mut stream).await?;
    stream.write_all(format!("P\t{game_code}\n").as_bytes()).await?;
    let _ = read_packet(&mut stream).await?;
    stream.write_all(b"C\n").await?;
    let char_resp = read_packet(&mut stream).await?;
    parse_character_list(&char_resp)
}

pub fn parse_character_list(resp: &str) -> Result<Vec<CharacterEntry>> {
    let parts: Vec<&str> = resp.trim().split('\t').collect();
    let skip = 5; // C + 4 numeric counts
    let mut entries = vec![];
    let mut i = skip;
    while i + 1 < parts.len() {
        let id = parts[i].to_string();
        let name = parts[i + 1].to_string();
        if !id.is_empty() && !name.is_empty() {
            entries.push(CharacterEntry { id, name });
        }
        i += 2;
    }
    Ok(entries)
}

/// Authenticate with the SGE eAccess server and return session credentials.
///
/// Follows the non-legacy protocol path from eaccess.rb:
///   K → A → M → F → G → P → C → L
pub async fn authenticate(
    account: &str,
    password: &str,
    game_code: &str,
    character_name: &str,
) -> Result<Session> {
    let connector = build_tls_connector()?;
    let tcp = TcpStream::connect((SGE_HOST, SGE_PORT)).await?;
    let mut stream = connector.connect(SGE_HOST, tcp).await?;

    // K — request hash key challenge
    stream.write_all(b"K\n").await?;
    let key = read_packet(&mut stream).await?;
    debug!("SGE key: {key}");

    // A — authenticate with hashed password
    // eaccess.rb line 93: password[i] = ((password[i] - 32) ^ hashkey[i]) + 32
    let hash = hash_password(password, &key);
    stream
        .write_all(format!("A\t{account}\t{hash}\n").as_bytes())
        .await?;
    let auth_resp = read_packet(&mut stream).await?;
    debug!("SGE auth: {auth_resp}");
    // eaccess.rb checks for /KEY\t/ match, raises AuthenticationError if not found
    if !auth_resp.contains("KEY\t") {
        let error_code = auth_resp.split_whitespace().last().unwrap_or("UNKNOWN");
        bail!("SGE auth failed ({}): {}", error_code, auth_resp);
    }

    // M — get game list (response must start with "M\t")
    stream.write_all(b"M\n").await?;
    let game_list = read_packet(&mut stream).await?;
    debug!("SGE game list: {game_list}");
    if !game_list.starts_with("M\t") {
        bail!("SGE M response unexpected: {game_list}");
    }

    // F — select game (response must match NORMAL|PREMIUM|TRIAL|INTERNAL|FREE)
    stream
        .write_all(format!("F\t{game_code}\n").as_bytes())
        .await?;
    let game_resp = read_packet(&mut stream).await?;
    debug!("SGE game select: {game_resp}");
    if !game_resp.contains("NORMAL")
        && !game_resp.contains("PREMIUM")
        && !game_resp.contains("TRIAL")
        && !game_resp.contains("INTERNAL")
        && !game_resp.contains("FREE")
    {
        bail!("SGE F response unexpected: {game_resp}");
    }

    // G — required step (eaccess.rb line 115-116), response ignored
    stream
        .write_all(format!("G\t{game_code}\n").as_bytes())
        .await?;
    let _g_resp = read_packet(&mut stream).await?;
    debug!("SGE G: {_g_resp}");

    // P — required step (eaccess.rb line 118-119), response ignored
    stream
        .write_all(format!("P\t{game_code}\n").as_bytes())
        .await?;
    let _p_resp = read_packet(&mut stream).await?;
    debug!("SGE P: {_p_resp}");

    // C — get character list
    stream.write_all(b"C\n").await?;
    let char_resp = read_packet(&mut stream).await?;
    debug!("SGE chars: {char_resp}");
    let char_id = find_character_id(&char_resp, character_name)?;

    // L — login as character
    stream
        .write_all(format!("L\t{char_id}\tSTORM\n").as_bytes())
        .await?;
    let session_resp = read_packet(&mut stream).await?;
    debug!("SGE session: {session_resp}");
    // eaccess.rb checks response =~ /^L\t/
    if !session_resp.starts_with("L\t") {
        bail!("SGE L response unexpected: {session_resp}");
    }

    parse_session(&session_resp, game_code, character_name)
}

/// Find a character's ID code in the C response.
///
/// From eaccess.rb lines 127-133:
///   Strip header: `response.sub(/^C\t[0-9]+\t[0-9]+\t[0-9]+\t[0-9]+[\t\n]/, '')`
///   Then scan pairs: `.scan(/[^\t]+\t[^\t^\n]+/)`
///   Find pair where `pair.split("\t")[1] == character`
///   Return `pair.split("\t")[0]` as char_code
///
/// The header is: "C\t<num>\t<num>\t<num>\t<num>\t" — 4 numeric fields after "C\t".
fn find_character_id(resp: &str, name: &str) -> Result<String> {
    // Strip the header: "C\t<digits>\t<digits>\t<digits>\t<digits>\t" or "\n"
    // Use a simple approach: split on "\t", skip "C" + 4 numeric fields = 5 elements.
    let parts: Vec<&str> = resp.trim().split('\t').collect();
    // parts[0] = "C", parts[1..4] = 4 numeric counts, parts[5..] = id\tname pairs
    // Verify: strip 5 leading fields (C + 4 numbers)
    let skip = 5;
    if parts.len() <= skip {
        bail!("Character '{name}' not found — C response too short: {resp}");
    }
    let name_lower = name.to_lowercase();
    let mut i = skip;
    while i + 1 < parts.len() {
        if parts[i + 1].to_lowercase() == name_lower {
            return Ok(parts[i].to_string());
        }
        i += 2;
    }
    bail!("Character '{name}' not found in character list. Response: {resp}")
}

/// Parse the L response into a Session.
///
/// From eaccess.rb lines 138-143:
///   Strip "L\tOK\t" prefix.
///   Split on "\t", then split each field on "=" into k/v pairs.
///   Keys are LOWERCASED: "gamehost", "gameport", "key".
///
/// Note: the plan used uppercase "GAMEHOST"/"GAMEPORT"/"KEY" — this is wrong.
/// The Ruby code explicitly calls `.downcase` on keys.
fn parse_session(resp: &str, game: &str, character: &str) -> Result<Session> {
    // Strip "L\tOK\t" prefix — eaccess.rb line 138
    let trimmed = resp.trim();
    let body = trimmed
        .strip_prefix("L\t")
        .and_then(|s| s.strip_prefix("OK\t"))
        .ok_or_else(|| anyhow::anyhow!("Unexpected L response format: {resp}"))?;

    let mut host = String::new();
    let mut port: u16 = 0;
    let mut key = String::new();
    let mut raw_fields: Vec<(String, String)> = Vec::new();

    for field in body.split('\t') {
        if let Some((k, v)) = field.split_once('=') {
            // Keys are lowercased per eaccess.rb line 142
            let k_lower = k.to_lowercase();
            raw_fields.push((k.to_string(), v.to_string()));
            match k_lower.as_str() {
                "gamehost" => host = v.to_string(),
                "gameport" => port = v.parse::<u16>().context("invalid gameport in SGE response")?,
                "key" => key = v.to_string(),
                _ => {}
            }
        }
    }

    if host.is_empty() || port == 0 || key.is_empty() {
        bail!("Incomplete session data from SGE. host={host:?} port={port} key_present={}. Raw: {resp}", !key.is_empty());
    }

    Ok(Session {
        host,
        port,
        key,
        game: game.to_string(),
        character: character.to_string(),
        raw_fields,
    })
}
