use crate::{game_obj::{GameObj, GameObjRegistry}, script_engine::ScriptEngine};
use mlua::prelude::*;
use std::sync::{Arc, Mutex};

/// Lua userdata wrapping a single game object.
/// Holds snapshot data for id/noun/name; status and contents are live lookups.
#[derive(Clone)]
pub struct LuaGameObj {
    pub id: String,
    pub noun: String,
    pub name: String,
    pub before_name: Option<String>,
    pub after_name: Option<String>,
    registry: Arc<Mutex<GameObjRegistry>>,
    type_data: Option<Arc<crate::type_data::TypeData>>,
}

impl LuaGameObj {
    fn from_obj(
        obj: &GameObj,
        registry: Arc<Mutex<GameObjRegistry>>,
        type_data: Option<Arc<crate::type_data::TypeData>>,
    ) -> Self {
        Self {
            id: obj.id.clone(),
            noun: obj.noun.clone(),
            name: obj.name.clone(),
            before_name: obj.before_name.clone(),
            after_name: obj.after_name.clone(),
            registry,
            type_data,
        }
    }
}

impl LuaUserData for LuaGameObj {
    fn add_fields<F: LuaUserDataFields<Self>>(fields: &mut F) {
        fields.add_field_method_get("id",   |_, this| Ok(this.id.clone()));
        fields.add_field_method_get("noun", |_, this| Ok(this.noun.clone()));
        fields.add_field_method_get("name", |_, this| Ok(this.name.clone()));
        fields.add_field_method_get("before_name", |_, this| Ok(this.before_name.clone()));
        fields.add_field_method_get("after_name",  |_, this| Ok(this.after_name.clone()));
        fields.add_field_method_get("full_name", |_, this| {
            Ok([this.before_name.as_deref(), Some(&this.name), this.after_name.as_deref()]
                .into_iter().flatten().collect::<Vec<_>>().join(" "))
        });

        // Live status read
        fields.add_field_method_get("status", |_, this| {
            Ok(this.registry.lock().unwrap().status(&this.id).to_string())
        });
        // Live status write
        fields.add_field_method_set("status", |_, this, val: String| {
            this.registry.lock().unwrap().set_status(&this.id, &val);
            Ok(())
        });

        // Live contents (container items).
        // Collect items while holding the lock, then drop the lock before calling into Lua
        // to avoid holding a Mutex guard during Lua allocations (GC safety).
        fields.add_field_method_get("contents", |lua, this| {
            let items: Option<Vec<GameObj>> = {
                let reg = this.registry.lock().unwrap();
                reg.contents.get(&this.id).cloned()
            };
            match items {
                None => Ok(LuaValue::Nil),
                Some(items) => {
                    let t = lua.create_table()?;
                    for (i, obj) in items.iter().enumerate() {
                        t.raw_set(i + 1, LuaGameObj::from_obj(obj, this.registry.clone(), this.type_data.clone()))?;
                    }
                    Ok(LuaValue::Table(t))
                }
            }
        });

        fields.add_field_method_get("type", |_, this| {
            match &this.type_data {
                Some(td) => Ok(td.get_type(&this.noun, &this.name).map(|s| s.to_string())),
                None => Ok(None),
            }
        });
        fields.add_field_method_get("sellable", |_, this| {
            match &this.type_data {
                Some(td) => Ok(td.get_sellable(&this.noun, &this.name).map(|s| s.to_string())),
                None => Ok(None),
            }
        });
    }

    fn add_methods<M: LuaUserDataMethods<Self>>(methods: &mut M) {
        methods.add_meta_method(LuaMetaMethod::ToString, |_, this, ()| {
            Ok(this.noun.clone())
        });
        methods.add_method("type_p", |_, this, tag: String| {
            match &this.type_data {
                Some(td) => Ok(td.is_type(&this.noun, &this.name, &tag)),
                None => Ok(false),
            }
        });
    }
}

/// Build a Lua table from a slice of `GameObj`s.
/// Note: mlua 0.10 `LuaTable` has no lifetime parameter.
fn obj_array(
    lua: &Lua,
    objs: &[GameObj],
    registry: Arc<Mutex<GameObjRegistry>>,
    type_data: Option<Arc<crate::type_data::TypeData>>,
) -> LuaResult<LuaTable> {
    let t = lua.create_table()?;
    for (i, obj) in objs.iter().enumerate() {
        t.raw_set(i + 1, LuaGameObj::from_obj(obj, registry.clone(), type_data.clone()))?;
    }
    Ok(t)
}

pub fn register(engine: &ScriptEngine) -> LuaResult<()> {
    let lua = &engine.lua;
    let go_arc = engine.game_objs.clone();
    let td_arc = engine.type_data.clone();

    let game_obj_tbl = lua.create_table()?;

    // GameObj.npcs()
    {
        let go = go_arc.clone();
        let td = td_arc.clone();
        game_obj_tbl.raw_set("npcs", lua.create_function(move |lua, ()| {
            let reg = go.lock().unwrap();
            let type_data = td.read().unwrap_or_else(|e| e.into_inner()).clone();
            match reg.as_ref() {
                None => Ok(lua.create_table()?),
                Some(r) => {
                    let r2 = r.lock().unwrap();
                    obj_array(lua, &r2.npcs, r.clone(), type_data)
                }
            }
        })?)?;
    }

    // GameObj.loot()
    {
        let go = go_arc.clone();
        let td = td_arc.clone();
        game_obj_tbl.raw_set("loot", lua.create_function(move |lua, ()| {
            let reg = go.lock().unwrap();
            let type_data = td.read().unwrap_or_else(|e| e.into_inner()).clone();
            match reg.as_ref() {
                None => Ok(lua.create_table()?),
                Some(r) => {
                    let r2 = r.lock().unwrap();
                    obj_array(lua, &r2.loot, r.clone(), type_data)
                }
            }
        })?)?;
    }

    // GameObj.pcs()
    {
        let go = go_arc.clone();
        let td = td_arc.clone();
        game_obj_tbl.raw_set("pcs", lua.create_function(move |lua, ()| {
            let reg = go.lock().unwrap();
            let type_data = td.read().unwrap_or_else(|e| e.into_inner()).clone();
            match reg.as_ref() {
                None => Ok(lua.create_table()?),
                Some(r) => {
                    let r2 = r.lock().unwrap();
                    obj_array(lua, &r2.pcs, r.clone(), type_data)
                }
            }
        })?)?;
    }

    // GameObj.inv()
    {
        let go = go_arc.clone();
        let td = td_arc.clone();
        game_obj_tbl.raw_set("inv", lua.create_function(move |lua, ()| {
            let reg = go.lock().unwrap();
            let type_data = td.read().unwrap_or_else(|e| e.into_inner()).clone();
            match reg.as_ref() {
                None => Ok(lua.create_table()?),
                Some(r) => {
                    let r2 = r.lock().unwrap();
                    obj_array(lua, &r2.inv, r.clone(), type_data)
                }
            }
        })?)?;
    }

    // GameObj.room_desc()
    {
        let go = go_arc.clone();
        let td = td_arc.clone();
        game_obj_tbl.raw_set("room_desc", lua.create_function(move |lua, ()| {
            let reg = go.lock().unwrap();
            let type_data = td.read().unwrap_or_else(|e| e.into_inner()).clone();
            match reg.as_ref() {
                None => Ok(lua.create_table()?),
                Some(r) => {
                    let r2 = r.lock().unwrap();
                    obj_array(lua, &r2.room_desc, r.clone(), type_data)
                }
            }
        })?)?;
    }

    // GameObj.right_hand()
    {
        let go = go_arc.clone();
        let td = td_arc.clone();
        game_obj_tbl.raw_set("right_hand", lua.create_function(move |lua, ()| {
            let reg = go.lock().unwrap();
            let type_data = td.read().unwrap_or_else(|e| e.into_inner()).clone();
            match reg.as_ref() {
                None => Ok(LuaValue::Nil),
                Some(r) => {
                    let r2 = r.lock().unwrap();
                    match &r2.right_hand {
                        None => Ok(LuaValue::Nil),
                        Some(obj) => Ok(LuaValue::UserData(lua.create_userdata(
                            LuaGameObj::from_obj(obj, r.clone(), type_data)
                        )?)),
                    }
                }
            }
        })?)?;
    }

    // GameObj.left_hand()
    {
        let go = go_arc.clone();
        let td = td_arc.clone();
        game_obj_tbl.raw_set("left_hand", lua.create_function(move |lua, ()| {
            let reg = go.lock().unwrap();
            let type_data = td.read().unwrap_or_else(|e| e.into_inner()).clone();
            match reg.as_ref() {
                None => Ok(LuaValue::Nil),
                Some(r) => {
                    let r2 = r.lock().unwrap();
                    match &r2.left_hand {
                        None => Ok(LuaValue::Nil),
                        Some(obj) => Ok(LuaValue::UserData(lua.create_userdata(
                            LuaGameObj::from_obj(obj, r.clone(), type_data)
                        )?)),
                    }
                }
            }
        })?)?;
    }

    // GameObj.fam_npcs()
    {
        let go = go_arc.clone();
        game_obj_tbl.raw_set("fam_npcs", lua.create_function(move |lua, ()| {
            let reg = go.lock().unwrap();
            match reg.as_ref() {
                None => Ok(lua.create_table()?),
                Some(r) => {
                    let r2 = r.lock().unwrap();
                    obj_array(lua, &r2.fam_npcs, r.clone(), None)
                }
            }
        })?)?;
    }

    // GameObj.fam_loot()
    {
        let go = go_arc.clone();
        game_obj_tbl.raw_set("fam_loot", lua.create_function(move |lua, ()| {
            let reg = go.lock().unwrap();
            match reg.as_ref() {
                None => Ok(lua.create_table()?),
                Some(r) => {
                    let r2 = r.lock().unwrap();
                    obj_array(lua, &r2.fam_loot, r.clone(), None)
                }
            }
        })?)?;
    }

    // GameObj.fam_pcs()
    {
        let go = go_arc.clone();
        game_obj_tbl.raw_set("fam_pcs", lua.create_function(move |lua, ()| {
            let reg = go.lock().unwrap();
            match reg.as_ref() {
                None => Ok(lua.create_table()?),
                Some(r) => {
                    let r2 = r.lock().unwrap();
                    obj_array(lua, &r2.fam_pcs, r.clone(), None)
                }
            }
        })?)?;
    }

    // GameObj.fam_room_desc()
    {
        let go = go_arc.clone();
        game_obj_tbl.raw_set("fam_room_desc", lua.create_function(move |lua, ()| {
            let reg = go.lock().unwrap();
            match reg.as_ref() {
                None => Ok(lua.create_table()?),
                Some(r) => {
                    let r2 = r.lock().unwrap();
                    obj_array(lua, &r2.fam_room_desc, r.clone(), None)
                }
            }
        })?)?;
    }

    // GameObj.targets() — NPCs that are valid targets (not dead)
    {
        let go = go_arc.clone();
        let td = td_arc.clone();
        game_obj_tbl.raw_set("targets", lua.create_function(move |lua, ()| {
            let reg = go.lock().unwrap();
            let type_data = td.read().unwrap_or_else(|e| e.into_inner()).clone();
            match reg.as_ref() {
                None => Ok(lua.create_table()?),
                Some(r) => {
                    let r2 = r.lock().unwrap();
                    let targets: Vec<GameObj> = r2.target_npcs().into_iter().cloned().collect();
                    obj_array(lua, &targets, r.clone(), type_data)
                }
            }
        })?)?;
    }

    // GameObj.target() — first valid target, or nil
    {
        let go = go_arc.clone();
        let td = td_arc.clone();
        game_obj_tbl.raw_set("target", lua.create_function(move |lua, ()| {
            let reg = go.lock().unwrap();
            let type_data = td.read().unwrap_or_else(|e| e.into_inner()).clone();
            match reg.as_ref() {
                None => Ok(LuaValue::Nil),
                Some(r) => {
                    let r2 = r.lock().unwrap();
                    match r2.target_npcs().first() {
                        None => Ok(LuaValue::Nil),
                        Some(obj) => Ok(LuaValue::UserData(lua.create_userdata(
                            LuaGameObj::from_obj(obj, r.clone(), type_data)
                        )?)),
                    }
                }
            }
        })?)?;
    }

    // GameObj.hidden_targets() — NPCs with status containing "hidden"
    {
        let go = go_arc.clone();
        let td = td_arc.clone();
        game_obj_tbl.raw_set("hidden_targets", lua.create_function(move |lua, ()| {
            let reg = go.lock().unwrap();
            let type_data = td.read().unwrap_or_else(|e| e.into_inner()).clone();
            match reg.as_ref() {
                None => Ok(lua.create_table()?),
                Some(r) => {
                    let r2 = r.lock().unwrap();
                    let hidden: Vec<GameObj> = r2.hidden_npcs().into_iter().cloned().collect();
                    obj_array(lua, &hidden, r.clone(), type_data)
                }
            }
        })?)?;
    }

    // GameObj.dead() — dead NPCs
    {
        let go = go_arc.clone();
        let td = td_arc.clone();
        game_obj_tbl.raw_set("dead", lua.create_function(move |lua, ()| {
            let reg = go.lock().unwrap();
            let type_data = td.read().unwrap_or_else(|e| e.into_inner()).clone();
            match reg.as_ref() {
                None => Ok(lua.create_table()?),
                Some(r) => {
                    let r2 = r.lock().unwrap();
                    let dead: Vec<GameObj> = r2.dead_npcs().into_iter().cloned().collect();
                    obj_array(lua, &dead, r.clone(), type_data)
                }
            }
        })?)?;
    }

    // GameObj.classify(noun, name) → type string or nil
    // Looks up type classification from gameobj-data.xml without needing a live object.
    {
        let td = td_arc.clone();
        game_obj_tbl.raw_set("classify", lua.create_function(move |_, (noun, name): (String, String)| {
            let type_data = td.read().unwrap_or_else(|e| e.into_inner());
            match type_data.as_ref() {
                Some(td) => Ok(td.get_type(&noun, &name).map(|s| s.to_string())),
                None => Ok(None),
            }
        })?)?;
    }

    // Metatable: GameObj["key"] — lookup by ID, noun, or name substring
    {
        let go = go_arc.clone();
        let td = td_arc.clone();
        let mt = lua.create_table()?;
        mt.raw_set("__index", lua.create_function(move |lua, (_t, key): (LuaTable, String)| {
            let reg = go.lock().unwrap();
            let type_data = td.read().unwrap_or_else(|e| e.into_inner()).clone();
            match reg.as_ref() {
                None => Ok(LuaValue::Nil),
                Some(r) => {
                    let r2 = r.lock().unwrap();
                    match r2.find(&key) {
                        None => Ok(LuaValue::Nil),
                        Some(obj) => Ok(LuaValue::UserData(lua.create_userdata(
                            LuaGameObj::from_obj(obj, r.clone(), type_data)
                        )?)),
                    }
                }
            }
        })?)?;
        game_obj_tbl.set_metatable(Some(mt));
    }

    lua.globals().raw_set("GameObj", game_obj_tbl)?;
    Ok(())
}
