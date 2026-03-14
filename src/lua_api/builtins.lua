-- builtins.lua: Lua-side built-in functions loaded at API registration time.
-- These compose Rust primitives (get, get_noblock, clear, pause, etc.).

function wait()
    clear()
    return get()
end

function matchfind(...)
    local patterns = {...}
    local lines = reget(100)
    for _, line in ipairs(lines) do
        for _, pattern in ipairs(patterns) do
            if string.find(line, pattern) then
                return line
            end
        end
    end
    return nil
end

function matchwait(...)
    local patterns = {...}
    while true do
        local line = get()
        for _, pattern in ipairs(patterns) do
            if string.find(line, pattern) then
                return line
            end
        end
    end
end

function matchtimeout(timeout, ...)
    local patterns = {...}
    local deadline = os.time() + timeout
    while os.time() < deadline do
        local line = get_noblock()
        if line then
            for _, pattern in ipairs(patterns) do
                if string.find(line, pattern) then
                    return line
                end
            end
        else
            pause(0.1)
        end
    end
    return nil
end

function checkrt()
    return math.max(0, GameState.roundtime())
end

function checkcastrt()
    return math.max(0, GameState.cast_roundtime())
end

function wait_until(func, interval)
    interval = interval or 0.1
    while true do
        local result = func()
        if result then return result end
        pause(interval)
    end
end

function wait_while(func, interval)
    interval = interval or 0.1
    while true do
        if not func() then return end
        pause(interval)
    end
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
