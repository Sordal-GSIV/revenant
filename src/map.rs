use anyhow::Result;
use serde::Deserialize;
use std::collections::{BinaryHeap, HashMap};
use std::cmp::Ordering;

#[derive(Debug, Clone, Deserialize)]
pub struct MapRoom {
    pub id: u32,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub description: String,
    /// wayto[dest_id_str] = command string to send
    #[serde(default)]
    pub wayto: HashMap<String, String>,
    /// timeto[dest_id_str] = travel weight (seconds); null means impassable without a script
    #[serde(default)]
    pub timeto: HashMap<String, Option<f64>>,
    #[serde(default)]
    pub paths: Vec<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub location: Option<String>,
    #[serde(default)]
    pub terrain: Option<String>,
    #[serde(default)]
    pub uid: Option<serde_json::Value>,  // string or array in the wild
    #[serde(default)]
    pub image: Option<String>,
    #[serde(default)]
    pub image_coords: Option<[f64; 4]>,
}

pub struct MapData {
    rooms: HashMap<u32, MapRoom>,
}

impl MapData {
    pub fn from_json(json: &str) -> Result<Self> {
        let rooms_vec: Vec<MapRoom> = serde_json::from_str(json)?;
        let rooms = rooms_vec.into_iter().map(|r| (r.id, r)).collect();
        Ok(Self { rooms })
    }

    pub fn from_file(path: &str) -> Result<Self> {
        let json = std::fs::read_to_string(path)?;
        Self::from_json(&json)
    }

    pub fn room_count(&self) -> usize {
        self.rooms.len()
    }

    pub fn get_room(&self, id: u32) -> Option<&MapRoom> {
        self.rooms.get(&id)
    }

    pub fn find_room_by_id(&self, id: u32) -> Option<&MapRoom> {
        self.rooms.get(&id)
    }

    /// Case-insensitive substring match on room title.
    pub fn find_room_by_name(&self, name: &str) -> Option<&MapRoom> {
        let lower = name.to_lowercase();
        self.rooms.values().find(|r| r.title.to_lowercase().contains(&lower))
    }

    /// Find room by tag (exact, case-insensitive).
    pub fn find_room_by_tag(&self, tag: &str) -> Option<&MapRoom> {
        let lower = tag.to_lowercase();
        self.rooms.values().find(|r| r.tags.iter().any(|t| t.to_lowercase() == lower))
    }

    /// Dijkstra shortest path from `from_id` to `to_id`.
    /// Returns `Some(vec_of_commands)` or `None` if unreachable.
    pub fn find_path(&self, from_id: u32, to_id: u32) -> Option<Vec<String>> {
        if from_id == to_id { return Some(vec![]); }
        if !self.rooms.contains_key(&from_id) || !self.rooms.contains_key(&to_id) {
            return None;
        }

        // dist[room_id] = (cost, prev_id, command_used)
        let mut dist: HashMap<u32, (f64, u32, String)> = HashMap::new();
        let mut heap: BinaryHeap<MinHeapEntry> = BinaryHeap::new();

        dist.insert(from_id, (0.0, from_id, String::new()));
        heap.push(MinHeapEntry { cost: 0.0, room_id: from_id });

        while let Some(MinHeapEntry { cost, room_id }) = heap.pop() {
            if room_id == to_id { break; }
            if let Some(&(best, _, _)) = dist.get(&room_id) {
                if cost > best { continue; } // stale entry
            }

            let room = match self.rooms.get(&room_id) { Some(r) => r, None => continue };
            for (dest_str, cmd) in &room.wayto {
                let dest_id: u32 = match dest_str.parse() { Ok(v) => v, Err(_) => continue };
                // timeto null means "impassable without a script" — skip this edge
                let edge_cost = match room.timeto.get(dest_str).copied().flatten() {
                    Some(c) => c,
                    None => continue, // null timeto = impassable, skip
                };
                let new_cost = cost + edge_cost;
                let better = dist.get(&dest_id).is_none_or(|&(c, _, _)| new_cost < c);
                if better {
                    dist.insert(dest_id, (new_cost, room_id, cmd.clone()));
                    heap.push(MinHeapEntry { cost: new_cost, room_id: dest_id });
                }
            }
        }

        // Reconstruct path
        if !dist.contains_key(&to_id) { return None; }
        let mut path = vec![];
        let mut cur = to_id;
        loop {
            let (_, prev, cmd) = dist.get(&cur)?;
            if *prev == cur { break; } // at start
            path.push(cmd.clone());
            cur = *prev;
        }
        path.reverse();
        if path.is_empty() { None } else { Some(path) }
    }

    /// Resolve a map UID to room IDs.
    /// UIDs can be stored as a number, string, or array in `MapRoom.uid`.
    pub fn ids_from_uid(&self, uid: u32) -> Vec<u32> {
        self.rooms.values()
            .filter(|r| match &r.uid {
                Some(serde_json::Value::Number(n)) => n.as_u64() == Some(uid as u64),
                Some(serde_json::Value::Array(arr)) => arr.iter().any(|v| v.as_u64() == Some(uid as u64)),
                Some(serde_json::Value::String(s)) => s.parse::<u32>().ok() == Some(uid),
                _ => false,
            })
            .map(|r| r.id)
            .collect()
    }

    /// Return sorted list of all room IDs.
    pub fn room_ids(&self) -> Vec<u32> {
        let mut ids: Vec<u32> = self.rooms.keys().copied().collect();
        ids.sort_unstable();
        ids
    }

    /// Find the nearest room with the given tag using Dijkstra.
    /// Returns (room_id, path_commands) or None if no tagged room is reachable.
    pub fn find_nearest_by_tag(&self, from_id: u32, tag: &str) -> Option<(u32, Vec<String>)> {
        if !self.rooms.contains_key(&from_id) { return None; }
        let tag_lower = tag.to_lowercase();

        // Check if current room has the tag
        if let Some(room) = self.rooms.get(&from_id) {
            if room.tags.iter().any(|t| t.to_lowercase() == tag_lower) {
                return Some((from_id, vec![]));
            }
        }

        // Dijkstra — same structure as find_path but stops at first tagged room
        let mut dist: HashMap<u32, (f64, u32, String)> = HashMap::new();
        let mut heap: BinaryHeap<MinHeapEntry> = BinaryHeap::new();

        dist.insert(from_id, (0.0, from_id, String::new()));
        heap.push(MinHeapEntry { cost: 0.0, room_id: from_id });

        while let Some(MinHeapEntry { cost, room_id }) = heap.pop() {
            if let Some(&(best, _, _)) = dist.get(&room_id) {
                if cost > best { continue; }
            }

            let room = match self.rooms.get(&room_id) { Some(r) => r, None => continue };
            for (dest_str, cmd) in &room.wayto {
                let dest_id: u32 = match dest_str.parse() { Ok(v) => v, Err(_) => continue };
                let edge_cost = match room.timeto.get(dest_str).copied().flatten() {
                    Some(c) => c,
                    None => continue,
                };
                let new_cost = cost + edge_cost;
                let better = dist.get(&dest_id).is_none_or(|&(c, _, _)| new_cost < c);
                if better {
                    dist.insert(dest_id, (new_cost, room_id, cmd.clone()));
                    heap.push(MinHeapEntry { cost: new_cost, room_id: dest_id });

                    // Check if destination has the tag
                    if let Some(dest_room) = self.rooms.get(&dest_id) {
                        if dest_room.tags.iter().any(|t| t.to_lowercase() == tag_lower) {
                            // Reconstruct path
                            let mut path = vec![];
                            let mut cur = dest_id;
                            loop {
                                let (_, prev, ref c) = dist.get(&cur)?;
                                if *prev == cur { break; }
                                path.push(c.clone());
                                cur = *prev;
                            }
                            path.reverse();
                            return Some((dest_id, path));
                        }
                    }
                }
            }
        }
        None
    }

    /// Find ALL rooms with a given tag, sorted by distance from `from_id`.
    /// Runs full Dijkstra, then collects tagged rooms sorted by cost.
    /// Returns Vec of (room_id, path_commands).
    pub fn find_all_nearest_by_tag(&self, from_id: u32, tag: &str) -> Vec<(u32, Vec<String>)> {
        if !self.rooms.contains_key(&from_id) { return vec![]; }
        let tag_lower = tag.to_lowercase();

        // Run full Dijkstra
        let mut dist: HashMap<u32, (f64, u32, String)> = HashMap::new();
        let mut heap: BinaryHeap<MinHeapEntry> = BinaryHeap::new();

        dist.insert(from_id, (0.0, from_id, String::new()));
        heap.push(MinHeapEntry { cost: 0.0, room_id: from_id });

        while let Some(MinHeapEntry { cost, room_id }) = heap.pop() {
            if let Some(&(best, _, _)) = dist.get(&room_id) {
                if cost > best { continue; }
            }
            let room = match self.rooms.get(&room_id) { Some(r) => r, None => continue };
            for (dest_str, cmd) in &room.wayto {
                let dest_id: u32 = match dest_str.parse() { Ok(v) => v, Err(_) => continue };
                let edge_cost = match room.timeto.get(dest_str).copied().flatten() {
                    Some(c) => c,
                    None => continue,
                };
                let new_cost = cost + edge_cost;
                let better = dist.get(&dest_id).is_none_or(|&(c, _, _)| new_cost < c);
                if better {
                    dist.insert(dest_id, (new_cost, room_id, cmd.clone()));
                    heap.push(MinHeapEntry { cost: new_cost, room_id: dest_id });
                }
            }
        }

        // Collect all tagged rooms with their costs
        let mut tagged: Vec<(u32, f64)> = Vec::new();
        for (&room_id, &(cost, _, _)) in &dist {
            if let Some(room) = self.rooms.get(&room_id) {
                if room.tags.iter().any(|t| t.to_lowercase() == tag_lower) {
                    tagged.push((room_id, cost));
                }
            }
        }
        tagged.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(Ordering::Equal));

        // Reconstruct paths
        tagged.iter().map(|&(room_id, _)| {
            if room_id == from_id {
                return (room_id, vec![]);
            }
            let mut path = vec![];
            let mut cur = room_id;
            loop {
                if let Some((_, prev, ref cmd)) = dist.get(&cur) {
                    if *prev == cur { break; }
                    path.push(cmd.clone());
                    cur = *prev;
                } else {
                    break;
                }
            }
            path.reverse();
            (room_id, path)
        }).collect()
    }

    /// Find the nearest room from a list of target room IDs using Dijkstra.
    /// Stops at the first target hit.
    /// Returns (room_id, path_commands) or None if no target is reachable.
    pub fn find_nearest_in_list(&self, from_id: u32, targets: &[u32]) -> Option<(u32, Vec<String>)> {
        if !self.rooms.contains_key(&from_id) { return None; }
        if targets.is_empty() { return None; }

        let target_set: std::collections::HashSet<u32> = targets.iter().copied().collect();

        // Check if we're already at a target
        if target_set.contains(&from_id) {
            return Some((from_id, vec![]));
        }

        let mut dist: HashMap<u32, (f64, u32, String)> = HashMap::new();
        let mut heap: BinaryHeap<MinHeapEntry> = BinaryHeap::new();

        dist.insert(from_id, (0.0, from_id, String::new()));
        heap.push(MinHeapEntry { cost: 0.0, room_id: from_id });

        while let Some(MinHeapEntry { cost, room_id }) = heap.pop() {
            if let Some(&(best, _, _)) = dist.get(&room_id) {
                if cost > best { continue; }
            }
            let room = match self.rooms.get(&room_id) { Some(r) => r, None => continue };
            for (dest_str, cmd) in &room.wayto {
                let dest_id: u32 = match dest_str.parse() { Ok(v) => v, Err(_) => continue };
                let edge_cost = match room.timeto.get(dest_str).copied().flatten() {
                    Some(c) => c,
                    None => continue,
                };
                let new_cost = cost + edge_cost;
                let better = dist.get(&dest_id).is_none_or(|&(c, _, _)| new_cost < c);
                if better {
                    dist.insert(dest_id, (new_cost, room_id, cmd.clone()));
                    heap.push(MinHeapEntry { cost: new_cost, room_id: dest_id });

                    // Check if destination is a target
                    if target_set.contains(&dest_id) {
                        let mut path = vec![];
                        let mut cur = dest_id;
                        loop {
                            let (_, prev, ref c) = dist.get(&cur)?;
                            if *prev == cur { break; }
                            path.push(c.clone());
                            cur = *prev;
                        }
                        path.reverse();
                        return Some((dest_id, path));
                    }
                }
            }
        }
        None
    }
}

/// Wrapper for BinaryHeap that implements min-heap ordering by cost.
#[derive(PartialEq)]
struct MinHeapEntry {
    cost: f64,
    room_id: u32,
}

impl Eq for MinHeapEntry {}

impl Ord for MinHeapEntry {
    fn cmp(&self, other: &Self) -> Ordering {
        // Reverse for min-heap
        other.cost.partial_cmp(&self.cost)
            .unwrap_or(Ordering::Equal)
            .then(self.room_id.cmp(&other.room_id))
    }
}

impl PartialOrd for MinHeapEntry {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}
