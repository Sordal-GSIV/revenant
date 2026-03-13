use revenant::game_obj::GameObjRegistry;

#[test]
fn test_new_npc_appears_in_npcs() {
    let mut reg = GameObjRegistry::new();
    reg.new_npc("-123", "goblin", "a snarling goblin", None);
    assert_eq!(reg.npcs.len(), 1);
    assert_eq!(reg.npcs[0].id, "-123");
    assert_eq!(reg.npcs[0].noun, "goblin");
    assert_eq!(reg.loot.len(), 0);
}

#[test]
fn test_new_loot_appears_in_loot() {
    let mut reg = GameObjRegistry::new();
    reg.new_loot("-456", "sword", "a rusty sword");
    assert_eq!(reg.loot.len(), 1);
    assert_eq!(reg.loot[0].name, "a rusty sword");
    assert_eq!(reg.npcs.len(), 0);
}

#[test]
fn test_deduplication_same_object_not_added_twice() {
    let mut reg = GameObjRegistry::new();
    reg.new_npc("-123", "goblin", "a snarling goblin", None);
    reg.new_npc("-123", "goblin", "a snarling goblin", None);
    assert_eq!(reg.npcs.len(), 1);
}

#[test]
fn test_clear_npcs_empties_registry_and_status() {
    let mut reg = GameObjRegistry::new();
    reg.new_npc("-123", "goblin", "a goblin", Some("stunned"));
    reg.clear_npcs();
    assert!(reg.npcs.is_empty());
    assert!(reg.npc_status.is_empty());
}

#[test]
fn test_status_returns_gone_for_unknown_id() {
    let reg = GameObjRegistry::new();
    assert_eq!(reg.status("-999"), "gone");
}

#[test]
fn test_npc_with_no_status_string_returns_gone() {
    // When an NPC is registered without a status string, npc_status contains no entry for it.
    // status() falls through to return "gone" — matching Lich5 which also returns 'gone' via
    // `@@npc_status[@id] || @@pc_status[@id] || 'gone'` (nil || 'gone' == 'gone' in Ruby).
    // Scripts distinguishing live vs absent NPCs should use GameObj.npcs() membership,
    // not status() alone.
    let mut reg = GameObjRegistry::new();
    reg.new_npc("-1", "goblin", "a goblin", None);
    assert_eq!(reg.status("-1"), "gone");
}

#[test]
fn test_npc_status_set_and_get() {
    let mut reg = GameObjRegistry::new();
    reg.new_npc("-123", "goblin", "a goblin", Some("prone"));
    assert_eq!(reg.status("-123"), "prone");
    reg.set_status("-123", "dead");
    assert_eq!(reg.status("-123"), "dead");
}

#[test]
fn test_set_status_only_affects_known_registry() {
    let mut reg = GameObjRegistry::new();
    reg.new_npc("-1", "goblin", "a goblin", None);
    // Try to set status on an NPC not in npcs (not in pcs either)
    reg.set_status("-999", "dead"); // should be a no-op
    assert!(!reg.npc_status.contains_key("-999"));
    assert!(!reg.pc_status.contains_key("-999"));
}

#[test]
fn test_right_hand_replaces_not_accumulates() {
    let mut reg = GameObjRegistry::new();
    reg.new_right_hand("-1", "sword", "a steel sword");
    reg.new_right_hand("-2", "axe", "a war axe");
    assert_eq!(reg.right_hand.as_ref().unwrap().noun, "axe");
}

#[test]
fn test_inv_with_container() {
    let mut reg = GameObjRegistry::new();
    reg.new_inv("-10", "backpack", "a leather backpack", None, None, None);
    reg.new_inv("-11", "coin", "a gold coin", Some("-10"), None, None);
    assert_eq!(reg.inv.len(), 1);
    assert_eq!(reg.inv[0].noun, "backpack");
    let contents = reg.contents.get("-10").unwrap();
    assert_eq!(contents.len(), 1);
    assert_eq!(contents[0].noun, "coin");
}

#[test]
fn test_find_by_id() {
    let mut reg = GameObjRegistry::new();
    reg.new_npc("-123", "goblin", "a snarling goblin", None);
    let found = reg.find("-123");
    assert!(found.is_some());
    assert_eq!(found.unwrap().noun, "goblin");
}

#[test]
fn test_find_by_noun() {
    let mut reg = GameObjRegistry::new();
    reg.new_loot("-456", "sword", "a rusty sword");
    assert!(reg.find("sword").is_some());
}

#[test]
fn test_find_by_name_substring() {
    let mut reg = GameObjRegistry::new();
    reg.new_loot("-456", "sword", "a rusty sword");
    assert!(reg.find("rusty").is_some());
}

#[test]
fn test_dead_npcs() {
    let mut reg = GameObjRegistry::new();
    reg.new_npc("-1", "goblin", "a goblin", None);
    reg.new_npc("-2", "troll", "a large troll", Some("dead"));
    let dead = reg.dead_npcs();
    assert_eq!(dead.len(), 1);
    assert_eq!(dead[0].id, "-2");
}

#[test]
fn test_clear_for_room_transition_clears_room_objects() {
    let mut reg = GameObjRegistry::new();
    reg.new_npc("-1", "goblin", "a goblin", None);
    reg.new_loot("-2", "sword", "a sword");
    reg.new_pc("12345", "Sordal", "Sordal Goldenleaf", None);
    reg.new_room_desc("-3", "tree", "a large oak");
    reg.clear_for_room_transition();
    assert!(reg.npcs.is_empty());
    assert!(reg.loot.is_empty());
    assert!(reg.pcs.is_empty());
    assert!(reg.room_desc.is_empty());
}

#[test]
fn test_clear_inv_also_clears_container_contents() {
    let mut reg = GameObjRegistry::new();
    reg.new_inv("-10", "backpack", "a leather backpack", None, None, None);
    reg.new_inv("-11", "coin", "a gold coin", Some("-10"), None, None);
    assert_eq!(reg.contents.get("-10").unwrap().len(), 1);
    reg.clear_inv();
    assert!(reg.inv.is_empty());
    assert!(reg.contents.is_empty(), "container contents should be cleared with inv");
}

#[test]
fn test_before_after_name_backfill() {
    let mut reg = GameObjRegistry::new();
    // First encounter without before/after
    reg.new_inv("-10", "potion", "a healing potion", None, None, None);
    // Second encounter with before/after fills in the blanks
    reg.new_inv("-10", "potion", "a healing potion", None, Some("(worn)"), Some("(glowing)"));
    let obj = reg.inv.iter().find(|o| o.id == "-10").unwrap();
    assert_eq!(obj.before_name.as_deref(), Some("(worn)"));
    assert_eq!(obj.after_name.as_deref(), Some("(glowing)"));
    // Ensure only one entry
    assert_eq!(reg.inv.len(), 1);
}
