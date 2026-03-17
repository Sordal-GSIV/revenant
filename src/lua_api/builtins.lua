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
    local elapsed = 0
    while elapsed < timeout do
        local line = get_noblock()
        if line then
            for _, pattern in ipairs(patterns) do
                if string.find(line, pattern) then
                    return line
                end
            end
        else
            pause(0.1)
            elapsed = elapsed + 0.1
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
    local elapsed = 0
    while true do
        if timeout and elapsed >= timeout then return nil end
        local line
        if timeout then
            line = get_noblock()
            if not line then
                pause(0.1)
                elapsed = elapsed + 0.1
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

-- Group 1: Threshold-checking vitals
-- No arg returns value; with arg returns value >= n
function checkmana(n)
    if n == nil then return GameState.mana end
    return GameState.mana >= n
end
function checkhealth(n)
    if n == nil then return GameState.health end
    return GameState.health >= n
end
function checkspirit(n)
    if n == nil then return GameState.spirit end
    return GameState.spirit >= n
end
function checkstamina(n)
    if n == nil then return GameState.stamina end
    return GameState.stamina >= n
end

-- Group 2: Percent vitals
-- No arg returns percent; with arg returns percent >= n
function percentmana(n)
    local p = Char.percent_mana
    if n == nil then return p end
    return p >= n
end
function percenthealth(n)
    local p = Char.percent_health
    if n == nil then return p end
    return p >= n
end
function percentspirit(n)
    local p = Char.percent_spirit
    if n == nil then return p end
    return p >= n
end
function percentstamina(n)
    local p = Char.percent_stamina
    if n == nil then return p end
    return p >= n
end
function percentconcentration(n)
    local max = GameState.max_concentration
    local p = max > 0 and math.floor(GameState.concentration * 100 / max) or 100
    if n == nil then return p end
    return p >= n
end
function percentstance(n)
    local v = GameState.stance_value
    if n == nil then return v end
    return v ~= nil and v >= n
end
function percentencumbrance(n)
    local v = GameState.encumbrance_value
    if n == nil then return v end
    return n <= v
end

-- Group 3: Mind-state checks
function checkmind(s)
    if s == nil then
        return GameState.mind
    elseif type(s) == "string" then
        return string.lower(GameState.mind):find(string.lower(s)) ~= nil
    elseif type(s) == "number" then
        local thresholds = {12, 25, 37, 50, 62, 75, 90, 100}
        local threshold = thresholds[math.floor(s)]
        if threshold == nil then return false end
        return GameState.mind_value >= threshold
    end
    return false
end
function percentmind(n)
    local v = GameState.mind_value
    if n == nil then return v end
    return v >= n
end
function checkfried()
    return GameState.mind_value >= 90
end
function checksaturated()
    return GameState.mind_value >= 100
end

-- Group 4: Compound status checks
function checkreallybleeding()
    return bleeding() and not (Spell.active_p(9909) or Spell.active_p(9905))
end
function muckled()
    return dead() or stunned() or webbed()
end

-- Room helper functions (only if Room and Map globals are registered)
if Room and Map then
    function Room.current()
        local id = Map.current_room()
        if id then return Map.find_room(id) end
        return nil
    end

    function Room.path_to(dest)
        local from = Map.current_room()
        if from then return Map.find_path(from, dest) end
        return nil
    end

    function Room.find_nearest_by_tag(tag)
        return Map.find_nearest_by_tag(tag)
    end
end

-- Group 5: Room checks
function checkarea(...)
    local args = {...}
    local title = Room.title or ""
    local area = title:match("^%[?([^,]+)") or title
    area = area:gsub("^%[", "")
    if #args == 0 then return area end
    local area_lower = area:lower()
    for _, pat in ipairs(args) do
        if area_lower:find(pat:lower(), 1, true) then return true end
    end
    return false
end
function checkroom(...)
    local args = {...}
    local title = Room.title or ""
    if #args == 0 then return title end
    for _, pat in ipairs(args) do
        if title:lower():find(pat:lower()) then return true end
    end
    return false
end
function checkroomdescrip(...)
    local args = {...}
    local desc = Room.description or ""
    if #args == 0 then return desc end
    for _, pat in ipairs(args) do
        if desc:lower():find(pat:lower()) then return true end
    end
    return false
end
function outside()
    local s = GameState.room_exits_string or ""
    return string.find(s, "Obvious paths:") ~= nil
end

-- Group 6: GameObj convenience checks
function checknpcs(...)
    local args = {...}
    local npcs = GameObj.npcs()
    if #npcs == 0 then
        if #args == 0 then return nil else return false end
    end
    local nouns = {}
    for _, npc in ipairs(npcs) do nouns[#nouns + 1] = npc.noun end
    if #args == 0 then return nouns end
    for _, pat in ipairs(args) do
        for _, noun in ipairs(nouns) do
            if noun:lower():find(pat:lower()) then return noun end
        end
    end
    return false
end
function checkpcs(...)
    local args = {...}
    local pcs = GameObj.pcs()
    if #pcs == 0 then
        if #args == 0 then return nil else return false end
    end
    local nouns = {}
    for _, pc in ipairs(pcs) do nouns[#nouns + 1] = pc.noun end
    if #args == 0 then return nouns end
    for _, pat in ipairs(args) do
        for _, noun in ipairs(nouns) do
            if noun:lower():find(pat:lower()) then return noun end
        end
    end
    return false
end
function checkloot()
    local loot = GameObj.loot()
    local nouns = {}
    for _, item in ipairs(loot) do nouns[#nouns + 1] = item.noun end
    return nouns
end
function checkright(...)
    local rh = GameObj.right_hand()
    if rh == nil then return nil end
    if rh.name == "Empty" or rh.name == "" then return nil end
    local args = {...}
    if #args == 0 then return rh.noun end
    for _, pat in ipairs(args) do
        if rh.name:lower():find(pat:lower()) then return pat end
    end
    return nil
end
function checkleft(...)
    local lh = GameObj.left_hand()
    if lh == nil then return nil end
    if lh.name == "Empty" or lh.name == "" then return nil end
    local args = {...}
    if #args == 0 then return lh.noun end
    for _, pat in ipairs(args) do
        if lh.name:lower():find(pat:lower()) then return pat end
    end
    return nil
end
function righthand_p()
    local rh = GameObj.right_hand()
    return rh ~= nil and rh.name ~= "Empty" and rh.name ~= ""
end
function lefthand_p()
    local lh = GameObj.left_hand()
    return lh ~= nil and lh.name ~= "Empty" and lh.name ~= ""
end

-- Group 7: Stance and encumbrance checks
function checkstance(val)
    if val == nil then return GameState.stance end
    if type(val) == "string" then
        local s = val:lower()
        if s:find("off") then return GameState.stance_value == 100
        elseif s:find("adv") then return GameState.stance_value ~= nil and GameState.stance_value >= 61 and GameState.stance_value <= 80
        elseif s:find("for") then return GameState.stance_value ~= nil and GameState.stance_value >= 41 and GameState.stance_value <= 60
        elseif s:find("neu") then return GameState.stance_value ~= nil and GameState.stance_value >= 21 and GameState.stance_value <= 40
        elseif s:find("gua") then return GameState.stance_value ~= nil and GameState.stance_value >= 1 and GameState.stance_value <= 20
        elseif s:find("def") then return GameState.stance_value == 0
        end
        return nil
    elseif type(val) == "number" then
        return GameState.stance_value == val
    end
    return nil
end
function checkencumbrance(val)
    if val == nil then return GameState.encumbrance end
    if type(val) == "number" then
        return val <= GameState.encumbrance_value
    elseif type(val) == "string" then
        return GameState.encumbrance:lower():find(val:lower()) ~= nil
    end
    return false
end

-- Group 8: Miscellaneous checks
function checkbounty()
    local task = Bounty.task
    if task == "" then return nil end
    return task
end
function checkspell(...)
    local nums = {...}
    if #nums == 0 then return false end
    for _, num in ipairs(nums) do
        if not Spell.active_p(num) then return false end
    end
    return true
end
function checkprep(spell)
    if spell == nil then return GameState.prepared_spell end
    local prep = GameState.prepared_spell
    if prep == nil then return false end
    return prep:lower():find(spell:lower()) ~= nil
end
function checkname(...)
    local args = {...}
    if #args == 0 then return GameState.name end
    for _, pat in ipairs(args) do
        if GameState.name:lower():find(pat:lower()) then return true end
    end
    return false
end

-- Group 9: Familiar checks
function checkfamroom(...)
    local args = {...}
    local title = Familiar.room_title or ""
    if #args == 0 then return title end
    for _, pat in ipairs(args) do
        if title:lower():find(pat:lower()) then return true end
    end
    return false
end
function checkfamarea(...)
    local args = {...}
    local title = Familiar.room_title or ""
    local area = title:match("^%[?([^,]+)") or title
    area = area:gsub("^%[", "")
    if #args == 0 then return area end
    for _, pat in ipairs(args) do
        if area:lower():find(pat:lower(), 1, true) then return true end
    end
    return false
end
function checkfampaths(dir)
    local exits = Familiar.room_exits
    if type(exits) ~= "table" then return false end
    if dir == nil or dir == "none" then
        if #exits == 0 then return false end
        return exits
    end
    for _, e in ipairs(exits) do
        if e == dir then return true end
    end
    return false
end
function checkfamnpcs(...)
    local args = {...}
    local npcs = GameObj.fam_npcs()
    local nouns = {}
    for _, npc in ipairs(npcs) do nouns[#nouns + 1] = npc.noun end
    if #nouns == 0 then return false end
    if #args == 0 then return nouns end
    for _, pat in ipairs(args) do
        for _, noun in ipairs(nouns) do
            if noun:lower():find(pat:lower()) then return noun end
        end
    end
    return false
end
function checkfampcs(...)
    local args = {...}
    local pcs = GameObj.fam_pcs()
    local nouns = {}
    for _, pc in ipairs(pcs) do nouns[#nouns + 1] = pc.noun end
    if #nouns == 0 then return false end
    if #args == 0 then return nouns end
    for _, pat in ipairs(args) do
        for _, noun in ipairs(nouns) do
            if noun:lower():find(pat:lower()) then return noun end
        end
    end
    return false
end
function checkfamroomdescrip(...)
    local args = {...}
    local desc = Familiar.room_description or ""
    if #args == 0 then return desc end
    for _, pat in ipairs(args) do
        if desc:lower():find(pat:lower()) then return true end
    end
    return false
end

-- Group 10: Movement and command patterns
function multimove(...)
    local dirs = {...}
    for _, dir in ipairs(dirs) do
        move(dir)
    end
end

function selectput(cmd, success, failure, timeout)
    if type(success) == "string" then success = {success} end
    if type(failure) == "string" then failure = {failure} end
    local start_time = os.time()
    while true do
        if timeout and (os.time() - start_time) >= timeout then return nil end
        fput(cmd)
        while true do
            local remaining = timeout and (timeout - (os.time() - start_time)) or nil
            if remaining and remaining <= 0 then return nil end
            local line = get_noblock()
            if not line then
                pause(0.1)
            else
                for _, pat in ipairs(success) do
                    if string.find(line, pat) then return line end
                end
                for _, pat in ipairs(failure) do
                    if string.find(line, pat) then break end
                end
                if string.find(line, "^>$") or string.find(line, "<prompt") then
                    break
                end
            end
        end
    end
end

-- dothistimeout(cmd, timeout_secs, patterns) → matched line or nil
-- Send a command, then read lines until one matches a pattern or timeout expires.
function dothistimeout(cmd, timeout, patterns)
    if type(patterns) == "string" then patterns = {patterns} end
    put(cmd)
    local start = os.time()
    while (os.time() - start) < timeout do
        local line = get_noblock()
        if line then
            for _, pat in ipairs(patterns) do
                if string.find(line, pat) then
                    return line
                end
            end
        else
            pause(0.1)
        end
    end
    return nil
end

-- Lich5 compatibility alias
Script.start = Script.run

-- Infomon CLI: intercept ;infomon commands upstream.
UpstreamHook.add("__infomon_cli", function(line)
    local cmd = line:match("^;infomon%s*(.*)$")
    if not cmd then return line end

    cmd = cmd:match("^%s*(.-)%s*$")  -- trim

    if cmd == "" or cmd == "help" then
        respond("Usage: ;infomon [sync | reset | show [full] | effects [true|false]]")
    elseif cmd == "sync" then
        respond("Infomon: syncing...")
        Infomon.sync()
        respond("Infomon: sync complete.")
    elseif cmd == "reset" then
        respond("Infomon: resetting database and syncing...")
        Infomon.reset()
        respond("Infomon: reset complete.")
    elseif cmd == "show" then
        Infomon.show(false)
    elseif cmd == "show full" then
        Infomon.show(true)
    elseif cmd:match("^effects") then
        local val = cmd:match("^effects%s+(%w+)")
        if val == "true" then
            Infomon.set_effects(true)
            respond("Infomon: effect durations enabled.")
        elseif val == "false" then
            Infomon.set_effects(false)
            respond("Infomon: effect durations disabled.")
        else
            local current = Infomon.effects()
            Infomon.set_effects(not current)
            respond("Infomon: effect durations " .. (not current and "enabled" or "disabled") .. ".")
        end
    else
        respond("Unknown infomon command: " .. cmd)
    end

    return ""  -- swallow the command
end)

-- Injured: true if body part has any wound or scar > 0
Injured = setmetatable({}, {
    __index = function(_, key)
        local w = Wounds[key]
        local s = Scars[key]
        if w == nil and s == nil then return nil end
        return (w or 0) > 0 or (s or 0) > 0
    end
})

-- Ephemeral session variables (gone on disconnect)
SessionVars = {}

-- Parse natural-language lists: "a kobold, a troll, and an ogre" → table
function parse_list(text)
    if not text or text == "" then return {} end
    -- "a kobold, a troll, and an ogre" → {"a kobold", "a troll", "an ogre"}
    text = text:gsub(",%s+and%s+", ", "):gsub("^and%s+", ""):gsub(",%s+", ",")
    local result = {}
    for item in text:gmatch("[^,]+") do
        item = item:match("^%s*(.-)%s*$")
        if item ~= "" then result[#result + 1] = item end
    end
    return result
end

-- Direction constants
SHORTDIR = { north="n", south="s", east="e", west="w", northeast="ne", southeast="se", southwest="sw", northwest="nw", up="u", down="d", out="out" }
LONGDIR = { n="north", s="south", e="east", w="west", ne="northeast", se="southeast", sw="southwest", nw="northwest", u="up", d="down" }
DIRMAP = { n="A", ne="B", e="C", se="D", s="E", sw="F", w="G", nw="H", up="I", down="J", out="K" }

-- Binary conversion utilities
function dec2bin(n)
    local result = ""
    while n > 0 do
        result = (n % 2) .. result
        n = math.floor(n / 2)
    end
    return result == "" and "0" or result
end

function bin2dec(s)
    return tonumber(s, 2) or 0
end

-- ============================================================
-- Client command buffer (ring buffer of last 100 upstream commands)
-- ============================================================
_CLIENT_BUFFER = {}
_CLIENT_BUFFER_MAX = 100
UpstreamHook.add("__client_buffer", function(line)
    _CLIENT_BUFFER[#_CLIENT_BUFFER + 1] = line
    if #_CLIENT_BUFFER > _CLIENT_BUFFER_MAX then
        table.remove(_CLIENT_BUFFER, 1)
    end
    return line
end)

-- ============================================================
-- Script.current — returns table with current script info
-- ============================================================
function Script.current()
    return {
        name = Script.name,
        paused = false,  -- a paused script cannot be executing this
    }
end

-- ============================================================
-- MINDMAP — mind state string → letter grade
-- ============================================================
MINDMAP = {
    ["clear as a bell"] = "A",
    ["fresh and clear"]  = "B",
    ["clear"]            = "C",
    ["muddled"]          = "D",
    ["becoming numbed"]  = "E",
    ["numbed"]           = "F",
    ["must rest"]        = "G",
    ["saturated"]        = "H",
}

-- ============================================================
-- ICONMAP — XML icon id → human-readable status name
-- ============================================================
ICONMAP = {
    ["IconBLEEDING"]  = "bleeding",
    ["IconSTUNNED"]   = "stunned",
    ["IconDEAD"]      = "dead",
    ["IconPOISONED"]  = "poisoned",
    ["IconDISEASED"]  = "diseased",
    ["IconSITTING"]   = "sitting",
    ["IconKNEELING"]  = "kneeling",
    ["IconPRONE"]     = "prone",
    ["IconSTANDING"]  = "standing",
    ["IconHIDDEN"]    = "hidden",
    ["IconINVISIBLE"] = "invisible",
    ["IconWEBBED"]    = "webbed",
    ["IconJOINED"]    = "joined",
}

-- ============================================================
-- xml_encode — global XML entity escaping
-- ============================================================
function xml_encode(text)
    return text:gsub("&", "&amp;"):gsub("<", "&lt;"):gsub(">", "&gt;"):gsub('"', "&quot;")
end

-- ============================================================
-- respond_to_window — send text to a specific stream window
-- ============================================================
function respond_to_window(window, text)
    respond("<pushStream id='" .. window .. "'/>" .. xml_encode(text) .. "<popStream id='" .. window .. "'/>")
end

-- ============================================================
-- Deprecated compat shims
-- ============================================================
function fetchloot()
    return GameObj.loot()
end

function take(item)
    fput("get " .. tostring(item))
end

function survivepoison()
    return not GameState.poisoned
end

function survivedisease()
    return not GameState.diseased
end

-- ============================================================
-- arrival_pcs — track PCs who arrived in the current room
-- ============================================================
_arrival_pcs = {}
local _last_room_count = 0
DownstreamHook.add("__arrival_pcs", function(line)
    if GameState.room_count ~= _last_room_count then
        _arrival_pcs = {}
        _last_room_count = GameState.room_count
    end
    local name = line:match("^(%u%a+) just arrived%.")
            or line:match("^(%u%a+) arrived%.")
    if name then
        _arrival_pcs[#_arrival_pcs + 1] = name
    end
    return line
end)

function arrival_pcs()
    return _arrival_pcs
end
