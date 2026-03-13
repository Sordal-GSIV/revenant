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
    /// timeto[dest_id_str] = travel weight (seconds)
    #[serde(default)]
    pub timeto: HashMap<String, f64>,
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
                let edge_cost = room.timeto.get(dest_str).copied().unwrap_or(1.0);
                let new_cost = cost + edge_cost;
                let better = dist.get(&dest_id).map_or(true, |&(c, _, _)| new_cost < c);
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
