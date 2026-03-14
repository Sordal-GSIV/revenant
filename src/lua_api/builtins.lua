-- builtins.lua: Lua-side built-in functions loaded at API registration time.
-- These compose Rust primitives (get, get_noblock, clear, pause, etc.).

function wait()
    clear()
    return get()
end

function waitforre(pattern, timeout)
    local deadline = timeout and (os.time() + timeout) or nil
    while true do
        if deadline and os.time() >= deadline then return nil end
        local line
        if deadline then
            line = get_noblock()
            if not line then
                pause(0.1)
            end
        else
            line = get()
        end
        if line then
            local captures = { string.match(line, pattern) }
            if #captures > 0 then return line, captures end
        end
    end
end
