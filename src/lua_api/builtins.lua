-- builtins.lua: Lua-side built-in functions loaded at API registration time.
-- These compose Rust primitives (get, get_noblock, clear, pause, etc.).

function wait()
    clear()
    return get()
end
