-- aliases.lua: Convenience globals matching Lich5 Ruby API.
-- Loaded via include_str!() at API registration time.

-- Vital stats
function health()           return GameState.health end
function max_health()       return GameState.max_health end
function mana()             return GameState.mana end
function max_mana()         return GameState.max_mana end
function spirit()           return GameState.spirit end
function max_spirit()       return GameState.max_spirit end
function stamina()          return GameState.stamina end
function max_stamina()      return GameState.max_stamina end
function concentration()    return GameState.concentration end
function max_concentration() return GameState.max_concentration end

-- Status predicates
function stunned()    return GameState.stunned end
function dead()       return GameState.dead end
function bleeding()   return GameState.bleeding end
function sleeping()   return GameState.sleeping end
function prone()      return GameState.prone end
function sitting()    return GameState.sitting end
function kneeling()   return GameState.kneeling end
function standing()   return GameState.standing end
function poisoned()   return GameState.poisoned end
function diseased()   return GameState.diseased end
function hidden()     return GameState.hidden end
function invisible()  return GameState.invisible end
function webbed()     return GameState.webbed end
function joined()     return GameState.joined end
function grouped()    return GameState.grouped end
function calmed()     return GameState.calmed end
function cutthroat()  return GameState.cutthroat end
function silenced()   return GameState.silenced end
function bound()      return GameState.bound end

-- Room info
function room_name()        return GameState.room_name end
function room_description() return GameState.room_description end

-- Roundtime
function roundtime()      return GameState.roundtime() end
function cast_roundtime() return GameState.cast_roundtime() end
