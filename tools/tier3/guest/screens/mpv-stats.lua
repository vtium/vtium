-- mpv script for ADR-103's measured sixel screens (acceptance.inc): when mpv
-- ends, writes to the file named by --script-opts=t3-stats=PATH how many
-- frames it played ("frames", output to the terminal) and how many it
-- dropped ("dropped", by the decoder or the video output, as it falls behind
-- the clip's 30 frames per second). Properties read at shutdown are already
-- reset, so the largest value seen while playing is kept.
local path = mp.get_opt("t3-stats")
local most = {}

-- estimated-frame-number sends no change notice, so the counts are polled.
local function poll()
    for _, name in ipairs({"estimated-frame-number", "frame-drop-count", "decoder-frame-drop-count"}) do
        local v = mp.get_property_number(name)
        if v ~= nil and (most[name] == nil or v > most[name]) then
            most[name] = v
        end
    end
end
mp.add_periodic_timer(0.02, poll)

mp.register_event("shutdown", function()
    if path == nil then
        return
    end
    local dropped = (most["frame-drop-count"] or 0) + (most["decoder-frame-drop-count"] or 0)
    local position = most["estimated-frame-number"]
    local seen = position and position + 1 or 0
    local f = io.open(path, "w")
    if f == nil then
        return
    end
    f:write(string.format("frames=%d\ndropped=%d\n", math.max(seen - dropped, 0), dropped))
    f:close()
end)
