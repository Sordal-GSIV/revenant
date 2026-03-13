use revenant::map::{MapData, MapRoom};

fn sample_json() -> &'static str {
    r#"[
      {"id":1,"title":"Town Square","wayto":{"2":"go north","3":"go east"},"timeto":{"2":0.2,"3":0.5},"paths":[],"tags":["town"]},
      {"id":2,"title":"North Road","wayto":{"1":"go south","4":"go north"},"timeto":{"1":0.2,"4":0.3},"paths":[],"tags":[]},
      {"id":3,"title":"East Gate","wayto":{"1":"go west"},"timeto":{"1":0.5},"paths":[],"tags":[]},
      {"id":4,"title":"Deep Forest","wayto":{"2":"go south"},"timeto":{"2":0.3},"paths":[],"tags":[]}
    ]"#
}

#[test]
fn test_load_from_json() {
    let data = MapData::from_json(sample_json()).unwrap();
    assert_eq!(data.room_count(), 4);
    assert_eq!(data.get_room(1).unwrap().title, "Town Square");
}

#[test]
fn test_dijkstra_direct() {
    let data = MapData::from_json(sample_json()).unwrap();
    let path = data.find_path(1, 2).unwrap();
    assert_eq!(path, vec!["go north"]);
}

#[test]
fn test_dijkstra_multi_hop() {
    let data = MapData::from_json(sample_json()).unwrap();
    let path = data.find_path(1, 4).unwrap();
    // 1→2 (go north) → 4 (go north)
    assert_eq!(path, vec!["go north", "go north"]);
}

#[test]
fn test_dijkstra_prefers_faster_route() {
    let data = MapData::from_json(sample_json()).unwrap();
    let path = data.find_path(1, 3).unwrap();
    assert_eq!(path, vec!["go east"]);
}

#[test]
fn test_find_room_by_id() {
    let data = MapData::from_json(sample_json()).unwrap();
    assert_eq!(data.find_room_by_id(3).unwrap().id, 3);
}

#[test]
fn test_find_room_by_name() {
    let data = MapData::from_json(sample_json()).unwrap();
    let room = data.find_room_by_name("north road").unwrap();
    assert_eq!(room.id, 2);
}

#[test]
fn test_no_path_returns_none() {
    let data = MapData::from_json(sample_json()).unwrap();
    // 3 (East Gate) only connects back to 1 — no path to 4
    assert!(data.find_path(3, 4).is_none());
}

#[test]
fn test_unknown_room_returns_none() {
    let data = MapData::from_json(sample_json()).unwrap();
    assert!(data.find_path(1, 999).is_none());
}
