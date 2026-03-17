use std::collections::HashMap;
use std::time::Instant;

/// A single game object (NPC, loot, PC, inventory item, etc.)
#[derive(Debug, Clone)]
pub struct GameObj {
    pub id: String,
    pub noun: String,
    pub name: String,
    pub before_name: Option<String>,
    pub after_name: Option<String>,
}

impl GameObj {
    fn new(id: &str, noun: &str, name: &str, before: Option<&str>, after: Option<&str>) -> Self {
        Self {
            id: id.to_string(),
            noun: normalize_noun(noun, name),
            name: name.to_string(),
            before_name: before.map(str::to_string),
            after_name: after.map(str::to_string),
        }
    }

    pub fn full_name(&self) -> String {
        [self.before_name.as_deref(), Some(self.name.as_str()), self.after_name.as_deref()]
            .into_iter()
            .flatten()
            .collect::<Vec<_>>()
            .join(" ")
    }
}

/// Lich5 normalizes several special nouns for reliable lookup (gameobj.rb lines 882-892).
/// GemStone sends long-form nouns for some items; Lich5 normalises them to short lookup keys.
fn normalize_noun(noun: &str, name: &str) -> String {
    match noun {
        "lapis lazuli"   => "lapis".to_string(),
        "Hammer of Kai"  => "hammer".to_string(),
        "ball and chain" => "ball".to_string(),
        "pearl" if name.contains("mother-of-pearl") => "mother-of-pearl".to_string(),
        _ => noun.to_string(),
    }
}

fn index_key(id: &str, noun: &str, name: &str) -> String {
    format!("{id}|{noun}|{name}")
}

/// Object category used when creating entries from the XML stream.
#[allow(dead_code)]
#[derive(Debug, Clone, PartialEq)]
pub enum ObjCategory {
    Loot,
    Npc,
    Pc,
    /// Inventory item; `container` is the ID of the enclosing container, or `None` for top-level.
    Inv { container: Option<String> },
    RoomDesc,
}

/// All live game objects for the current room/session.
#[derive(Default)]
pub struct GameObjRegistry {
    pub loot: Vec<GameObj>,
    pub npcs: Vec<GameObj>,
    pub npc_status: HashMap<String, String>,
    pub pcs: Vec<GameObj>,
    pub pc_status: HashMap<String, String>,
    pub inv: Vec<GameObj>,
    pub contents: HashMap<String, Vec<GameObj>>,
    pub right_hand: Option<GameObj>,
    pub left_hand: Option<GameObj>,
    pub room_desc: Vec<GameObj>,
    /// Familiar's room objects — parallel registries for the familiar's perspective.
    pub fam_loot: Vec<GameObj>,
    pub fam_npcs: Vec<GameObj>,
    pub fam_pcs: Vec<GameObj>,
    pub fam_room_desc: Vec<GameObj>,
    /// Deduplication index: composite key → canonical GameObj instance.
    /// Persists across room transitions so the same object reuses the same instance.
    index: HashMap<String, GameObj>,
    /// Tracks when each index key was last referenced (for TTL-based GC).
    last_seen: HashMap<String, Instant>,
}

impl GameObjRegistry {
    pub fn new() -> Self { Self::default() }

    // ── Factory methods ──────────────────────────────────────────────────────

    pub fn new_npc(&mut self, id: &str, noun: &str, name: &str, status: Option<&str>) {
        if let Some(s) = status {
            self.npc_status.insert(id.to_string(), s.to_string());
        }
        let obj = self.find_or_create(id, noun, name, None, None);
        if let Some(existing) = self.npcs.iter_mut().find(|o| o.id == id) {
            *existing = obj;
        } else {
            self.npcs.push(obj);
        }
    }

    pub fn new_loot(&mut self, id: &str, noun: &str, name: &str) {
        let obj = self.find_or_create(id, noun, name, None, None);
        if let Some(existing) = self.loot.iter_mut().find(|o| o.id == id) {
            *existing = obj;
        } else {
            self.loot.push(obj);
        }
    }

    pub fn new_pc(&mut self, id: &str, noun: &str, name: &str, status: Option<&str>) {
        if let Some(s) = status {
            self.pc_status.insert(id.to_string(), s.to_string());
        }
        let obj = self.find_or_create(id, noun, name, None, None);
        if let Some(existing) = self.pcs.iter_mut().find(|o| o.id == id) {
            *existing = obj;
        } else {
            self.pcs.push(obj);
        }
    }

    pub fn new_inv(
        &mut self, id: &str, noun: &str, name: &str,
        container: Option<&str>, before: Option<&str>, after: Option<&str>,
    ) {
        let obj = self.find_or_create(id, noun, name, before, after);
        if let Some(cid) = container {
            let items = self.contents.entry(cid.to_string()).or_default();
            if let Some(existing) = items.iter_mut().find(|o| o.id == id) {
                *existing = obj;
            } else {
                items.push(obj);
            }
        } else if let Some(existing) = self.inv.iter_mut().find(|o| o.id == id) {
            *existing = obj;
        } else {
            self.inv.push(obj);
        }
    }

    pub fn new_right_hand(&mut self, id: &str, noun: &str, name: &str) {
        let obj = self.find_or_create(id, noun, name, None, None);
        self.right_hand = Some(obj);
    }

    pub fn new_left_hand(&mut self, id: &str, noun: &str, name: &str) {
        let obj = self.find_or_create(id, noun, name, None, None);
        self.left_hand = Some(obj);
    }

    #[allow(dead_code)]
    pub fn new_room_desc(&mut self, id: &str, noun: &str, name: &str) {
        let obj = self.find_or_create(id, noun, name, None, None);
        if let Some(existing) = self.room_desc.iter_mut().find(|o| o.id == id) {
            *existing = obj;
        } else {
            self.room_desc.push(obj);
        }
    }

    pub fn new_fam_npc(&mut self, id: &str, noun: &str, name: &str) {
        let obj = self.find_or_create(id, noun, name, None, None);
        if let Some(existing) = self.fam_npcs.iter_mut().find(|o| o.id == id) {
            *existing = obj;
        } else {
            self.fam_npcs.push(obj);
        }
    }

    pub fn new_fam_loot(&mut self, id: &str, noun: &str, name: &str) {
        let obj = self.find_or_create(id, noun, name, None, None);
        if let Some(existing) = self.fam_loot.iter_mut().find(|o| o.id == id) {
            *existing = obj;
        } else {
            self.fam_loot.push(obj);
        }
    }

    pub fn new_fam_pc(&mut self, id: &str, noun: &str, name: &str) {
        let obj = self.find_or_create(id, noun, name, None, None);
        if let Some(existing) = self.fam_pcs.iter_mut().find(|o| o.id == id) {
            *existing = obj;
        } else {
            self.fam_pcs.push(obj);
        }
    }

    pub fn new_fam_room_desc(&mut self, id: &str, noun: &str, name: &str) {
        let obj = self.find_or_create(id, noun, name, None, None);
        if let Some(existing) = self.fam_room_desc.iter_mut().find(|o| o.id == id) {
            *existing = obj;
        } else {
            self.fam_room_desc.push(obj);
        }
    }

    // ── Clear methods ────────────────────────────────────────────────────────

    pub fn clear_loot(&mut self) { self.loot.clear(); }
    pub fn clear_npcs(&mut self) { self.npcs.clear(); self.npc_status.clear(); }
    pub fn clear_pcs(&mut self) { self.pcs.clear(); self.pc_status.clear(); }
    pub fn clear_inv(&mut self) { self.inv.clear(); self.contents.clear(); }
    pub fn clear_room_desc(&mut self) { self.room_desc.clear(); }
    pub fn clear_all_containers(&mut self) { self.contents.clear(); }

    pub fn clear_fam_loot(&mut self) { self.fam_loot.clear(); }
    pub fn clear_fam_npcs(&mut self) { self.fam_npcs.clear(); }
    pub fn clear_fam_pcs(&mut self) { self.fam_pcs.clear(); }
    pub fn clear_fam_room_desc(&mut self) { self.fam_room_desc.clear(); }

    pub fn clear_familiar(&mut self) {
        self.clear_fam_loot();
        self.clear_fam_npcs();
        self.clear_fam_pcs();
        self.clear_fam_room_desc();
    }

    /// Called on `<nav rm="...">` — clears all room-scoped registries.
    /// The deduplication index is intentionally preserved so re-encountered
    /// objects reuse existing instances (Lich5 `find_or_create` behaviour).
    /// Stale index entries older than 5 minutes are pruned on each transition.
    pub fn clear_for_room_transition(&mut self) {
        self.clear_loot();
        self.clear_npcs();
        self.clear_pcs();
        self.clear_room_desc();
        self.prune_stale();
    }

    // ── Status ───────────────────────────────────────────────────────────────

    pub fn status(&self, id: &str) -> &str {
        self.npc_status.get(id)
            .or_else(|| self.pc_status.get(id))
            .map(String::as_str)
            .unwrap_or("gone")
    }

    /// Update status; silently ignores IDs not in any known registry.
    pub fn set_status(&mut self, id: &str, status: &str) {
        if self.npcs.iter().any(|o| o.id == id) {
            self.npc_status.insert(id.to_string(), status.to_string());
        } else if self.pcs.iter().any(|o| o.id == id) {
            self.pc_status.insert(id.to_string(), status.to_string());
        }
    }

    // ── Lookup ───────────────────────────────────────────────────────────────

    /// Find by exact ID, then noun, then name substring.
    /// Search order: inv → loot → npcs → pcs → hands → room_desc → container contents.
    pub fn find(&self, val: &str) -> Option<&GameObj> {
        let all: Vec<&GameObj> = self.inv.iter()
            .chain(&self.loot)
            .chain(&self.npcs)
            .chain(&self.pcs)
            .chain(self.right_hand.iter())
            .chain(self.left_hand.iter())
            .chain(&self.room_desc)
            .chain(self.contents.values().flatten())
            .collect();

        all.iter().copied().find(|o| o.id == val)
            .or_else(|| all.iter().copied().find(|o| o.noun == val))
            .or_else(|| all.iter().copied().find(|o| o.name.contains(val)))
    }

    /// NPCs whose status is "dead".
    pub fn dead_npcs(&self) -> Vec<&GameObj> {
        self.npcs.iter()
            .filter(|o| self.npc_status.get(&o.id).map(|s| s == "dead").unwrap_or(false))
            .collect()
    }

    /// NPCs that are valid targets: status is NOT "dead" (and status is not absent
    /// with a "dead" substring either — mirrors Lich5 `GameObj.targets`).
    pub fn target_npcs(&self) -> Vec<&GameObj> {
        self.npcs.iter()
            .filter(|o| {
                match self.npc_status.get(&o.id) {
                    Some(s) => !s.contains("dead"),
                    None => true, // no status means alive
                }
            })
            .collect()
    }

    /// NPCs whose status contains "hidden".
    pub fn hidden_npcs(&self) -> Vec<&GameObj> {
        self.npcs.iter()
            .filter(|o| self.npc_status.get(&o.id).map(|s| s.contains("hidden")).unwrap_or(false))
            .collect()
    }

    // ── TTL-based GC ─────────────────────────────────────────────────────────

    /// Default TTL for stale index entries: 5 minutes.
    const STALE_TTL_SECS: u64 = 300;

    /// Remove deduplication index entries that haven't been referenced within
    /// `ttl` (defaults to 300 s). Call on room transitions or periodically.
    pub fn prune_stale(&mut self) {
        let cutoff = Instant::now() - std::time::Duration::from_secs(Self::STALE_TTL_SECS);
        let stale_keys: Vec<String> = self.last_seen.iter()
            .filter(|(_, &ts)| ts < cutoff)
            .map(|(k, _)| k.clone())
            .collect();
        for key in &stale_keys {
            self.index.remove(key);
            self.last_seen.remove(key);
        }
    }

    // ── Deduplication index ──────────────────────────────────────────────────

    fn find_or_create(
        &mut self, id: &str, noun: &str, name: &str,
        before: Option<&str>, after: Option<&str>,
    ) -> GameObj {
        let key = index_key(id, noun, name);
        self.last_seen.insert(key.clone(), Instant::now());
        if let Some(existing) = self.index.get_mut(&key) {
            if existing.before_name.is_none() {
                existing.before_name = before.map(str::to_string);
            }
            if existing.after_name.is_none() {
                existing.after_name = after.map(str::to_string);
            }
            existing.clone()
        } else {
            let obj = GameObj::new(id, noun, name, before, after);
            self.index.insert(key, obj.clone());
            obj
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fam_npc_creation() {
        let mut reg = GameObjRegistry::new();
        reg.new_fam_npc("100", "kobold", "a kobold");
        assert_eq!(reg.fam_npcs.len(), 1);
        assert_eq!(reg.fam_npcs[0].noun, "kobold");
    }

    #[test]
    fn test_fam_registries_clear() {
        let mut reg = GameObjRegistry::new();
        reg.new_fam_npc("100", "kobold", "a kobold");
        reg.new_fam_loot("200", "chest", "a chest");
        reg.new_fam_pc("300", "Gandalf", "Gandalf");
        reg.new_fam_room_desc("400", "sign", "a sign");
        reg.clear_familiar();
        assert!(reg.fam_npcs.is_empty());
        assert!(reg.fam_loot.is_empty());
        assert!(reg.fam_pcs.is_empty());
        assert!(reg.fam_room_desc.is_empty());
    }

    #[test]
    fn test_fam_npc_dedup() {
        let mut reg = GameObjRegistry::new();
        reg.new_fam_npc("100", "kobold", "a kobold");
        reg.new_fam_npc("100", "kobold", "a kobold");
        assert_eq!(reg.fam_npcs.len(), 1);
    }

    #[test]
    fn test_prune_stale_removes_old_entries() {
        let mut reg = GameObjRegistry::new();
        reg.new_npc("100", "kobold", "a kobold", None);
        assert_eq!(reg.index.len(), 1);
        assert_eq!(reg.last_seen.len(), 1);

        // Manually backdate the timestamp to make it stale
        let key = index_key("100", "kobold", "a kobold");
        reg.last_seen.insert(key, Instant::now() - std::time::Duration::from_secs(400));

        reg.prune_stale();
        assert!(reg.index.is_empty());
        assert!(reg.last_seen.is_empty());
    }

    #[test]
    fn test_prune_stale_keeps_recent_entries() {
        let mut reg = GameObjRegistry::new();
        reg.new_npc("100", "kobold", "a kobold", None);
        // Just created — should not be pruned
        reg.prune_stale();
        assert_eq!(reg.index.len(), 1);
    }
}
