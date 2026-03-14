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

function move(direction)
    local max_retries = 10
    for attempt = 1, max_retries do
        waitrt()
        while GameState.stunned do pause(0.5) end
        _raw_fput(direction)
        -- Check for success or failure in the output
        for _ = 1, 20 do
            local line = get_noblock()
            if not line then
                pause(0.1)
            else
                if string.find(line, "obvious exits") or string.find(line, "Obvious exits")
                   or string.find(line, "Obvious paths") or string.find(line, "obvious paths") then
                    return true
                elseif string.find(line, "%.%.%.wait") or string.find(line, "you can't do that")
                   or string.find(line, "You can't do that") then
                    pause(0.5)
                    break -- retry
                elseif string.find(line, "stunned") then
                    while GameState.stunned do pause(0.5) end
                    break -- retry
                elseif string.find(line, "webbed") then
                    pause(3)
                    break -- retry
                end
            end
        end
    end
    error("move(" .. direction .. ") failed after " .. max_retries .. " attempts")
end

-- Direction shortcuts
function n()   return move("north") end
function s()   return move("south") end
function e()   return move("east") end
function w()   return move("west") end
function ne()  return move("northeast") end
function se()  return move("southeast") end
function sw()  return move("southwest") end
function nw()  return move("northwest") end
function u()   return move("up") end
function d()   return move("down") end
function out() return move("out") end

function fput(cmd, ...)
    local waitingfor = {...}
    -- Wait for roundtime
    local rt = checkrt()
    if rt > 0 then pause(rt + 0.3) end
    -- Wait for stun
    while GameState.stunned do pause(0.5) end
    if #waitingfor == 0 then
        -- No patterns: send and wait for prompt (original behavior)
        _raw_fput(cmd)
    else
        -- Retry until one of the waitingfor patterns appears
        while true do
            _raw_fput(cmd)
            while true do
                local line = get()
                for _, pattern in ipairs(waitingfor) do
                    if string.find(line, pattern) then return line end
                end
                -- If we hit a prompt without matching, break to resend
                if string.find(line, "^>$") or string.find(line, "<prompt") then
                    break
                end
            end
        end
    end
end

function multifput(...)
    for _, cmd in ipairs({...}) do
        fput(cmd)
    end
end

function die_with_me(target)
    before_dying(function()
        Script.kill(target)
    end)
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
