-- Dyrskthota in Limsa Lominsa

-- Scenes
SCENE_MENU = 00000 -- Opens route table
SCENE_NO_QUEST = 00001 -- I think this is the message that plays if you haven't unlocked Ocean Fishing yet, unsure
SCENE_00002 = 00002
SCENE_00003 = 00003 -- We are no longer recruiting crew members
SCENE_OPEN_BOARDING_GREETING = 00004 -- Greeting when the boarding is open

-- Hardcoded for now
ROUTE1 = 1 -- Northern Strain of Merithor
ROUTE2 = 19 -- Thavnairian coast

function onTalk(target, player)
    -- Always show the boarding greeting for now
    player:play_scene(SCENE_OPEN_BOARDING_GREETING, HIDE_HOTBAR, {})
end

function onReturn(scene, results, player)
    if scene == SCENE_OPEN_BOARDING_GREETING then
        -- Show the route menu
        player:play_scene(SCENE_MENU, HIDE_HOTBAR, {1, 0})
        return
    end
    if scene == SCENE_MENU then
        if results[1] == 2 then
            local stillRecruitingMembers = 1 -- We are no longer recruiting members message
            local canReboard = 1 -- Cannot allow to board back on the ship
            player:play_scene(SCENE_00002, HIDE_HOTBAR, {stillRecruitingMembers, canReboard, ROUTE1, ROUTE2})
            return
        elseif results[1] == 4 then
            -- Show help menu (second param)
            local target_event_id = results[2]
            player:start_event(target_event_id, EVENT_TYPE_NEST, 0)
            player:play_scene(0, HIDE_HOTBAR | NO_DEFAULT_CAMERA, {})

            return
        end
    end
    player:finish_event()
end

function onYield(scene, id, results, player)
    local chosenRoute = results[1]
    local route
    if chosenRoute == 0 then
        route = ROUTE1
    else
        route = ROUTE2
    end

    player:register_for_content(GAME_DATA:lookup_ikd_route_content(route))
    player:finish_event()
end
