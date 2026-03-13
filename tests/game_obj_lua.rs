use revenant::{game_obj::GameObjRegistry, script_engine::ScriptEngine};
use std::sync::{Arc, Mutex};

fn make_engine_with_objs() -> (Arc<ScriptEngine>, Arc<Mutex<GameObjRegistry>>) {
    let engine = Arc::new(ScriptEngine::new());
    engine.install_lua_api().unwrap();
    let go = Arc::new(Mutex::new(GameObjRegistry::new()));
    engine.set_game_objs(go.clone());
    (engine, go)
}

#[tokio::test]
async fn test_gameobj_npcs_accessible_from_lua() {
    let (engine, go) = make_engine_with_objs();
    go.lock().unwrap().new_npc("-123", "goblin", "a snarling goblin", None);

    engine.set_script_error_hook(|_, e| panic!("Lua error: {e}"));

    engine.eval_lua(r#"
        local npcs = GameObj.npcs()
        assert(#npcs == 1, "expected 1 NPC, got " .. #npcs)
        assert(npcs[1].noun == "goblin", "expected goblin, got " .. tostring(npcs[1].noun))
        assert(npcs[1].id == "-123", "expected -123, got " .. tostring(npcs[1].id))
    "#).await.unwrap();
}

#[tokio::test]
async fn test_gameobj_loot_accessible_from_lua() {
    let (engine, go) = make_engine_with_objs();
    go.lock().unwrap().new_loot("-456", "sword", "a rusty sword");

    engine.set_script_error_hook(|_, e| panic!("Lua error: {e}"));
    engine.eval_lua(r#"
        local loot = GameObj.loot()
        assert(#loot == 1)
        assert(loot[1].name == "a rusty sword")
    "#).await.unwrap();
}

#[tokio::test]
async fn test_gameobj_index_lookup_by_noun() {
    let (engine, go) = make_engine_with_objs();
    go.lock().unwrap().new_loot("-1", "sword", "a rusty sword");

    engine.set_script_error_hook(|_, e| panic!("Lua error: {e}"));
    engine.eval_lua(r#"
        local obj = GameObj["sword"]
        assert(obj ~= nil, "expected object by noun lookup")
        assert(obj.noun == "sword")
    "#).await.unwrap();
}

#[tokio::test]
async fn test_gameobj_status_live_read_write() {
    let (engine, go) = make_engine_with_objs();
    go.lock().unwrap().new_npc("-1", "goblin", "a goblin", None);

    engine.set_script_error_hook(|_, e| panic!("Lua error: {e}"));
    engine.eval_lua(r#"
        local npcs = GameObj.npcs()
        assert(npcs[1].status == "gone", "default status should be gone")
        npcs[1].status = "dead"
        assert(npcs[1].status == "dead", "status should update to dead")
    "#).await.unwrap();
}

#[tokio::test]
async fn test_gameobj_dead_returns_dead_npcs() {
    let (engine, go) = make_engine_with_objs();
    {
        let mut reg = go.lock().unwrap();
        reg.new_npc("-1", "goblin", "a goblin", Some("dead"));
        reg.new_npc("-2", "troll", "a troll", None);
    }

    engine.set_script_error_hook(|_, e| panic!("Lua error: {e}"));
    engine.eval_lua(r#"
        local dead = GameObj.dead()
        assert(#dead == 1, "expected 1 dead NPC, got " .. #dead)
        assert(dead[1].noun == "goblin")
    "#).await.unwrap();
}

#[tokio::test]
async fn test_gameobj_right_hand() {
    let (engine, go) = make_engine_with_objs();
    go.lock().unwrap().new_right_hand("-5", "sword", "a steel sword");

    engine.set_script_error_hook(|_, e| panic!("Lua error: {e}"));
    engine.eval_lua(r#"
        local rh = GameObj.right_hand()
        assert(rh ~= nil, "right hand should not be nil")
        assert(rh.noun == "sword")
    "#).await.unwrap();
}

#[tokio::test]
async fn test_gameobj_empty_hand_returns_nil() {
    let (engine, _go) = make_engine_with_objs();

    engine.set_script_error_hook(|_, e| panic!("Lua error: {e}"));
    engine.eval_lua(r#"
        local rh = GameObj.right_hand()
        assert(rh == nil, "empty hand should return nil")
    "#).await.unwrap();
}
