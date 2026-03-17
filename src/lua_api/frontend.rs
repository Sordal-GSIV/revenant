use crate::script_engine::ScriptEngine;

pub fn register(engine: &ScriptEngine) -> mlua::Result<()> {
    let lua = &engine.lua;
    let t = lua.create_table()?;

    let fe = engine.frontend.clone();
    t.set("name", {
        let fe = fe.clone();
        lua.create_function(move |_, ()| {
            Ok(fe.lock().unwrap().as_str().to_string())
        })?
    })?;

    // supports_xml, supports_gsl, supports_streams, supports_mono, supports_room_window
    let cap_fns = [
        ("supports_xml", crate::frontend::Capability::Xml),
        ("supports_gsl", crate::frontend::Capability::Gsl),
        ("supports_streams", crate::frontend::Capability::Streams),
        ("supports_mono", crate::frontend::Capability::Mono),
        ("supports_room_window", crate::frontend::Capability::RoomWindow),
    ];

    for (name, cap) in cap_fns {
        let fe = fe.clone();
        t.set(name, lua.create_function(move |_, ()| {
            Ok(fe.lock().unwrap().supports(cap))
        })?)?;
    }

    lua.globals().set("Frontend", t)?;
    Ok(())
}
