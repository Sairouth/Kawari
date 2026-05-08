//! Quests!

use crate::{ZoneConnection, inventory::Storage, zone_connection::PersistentQuest};
use kawari::{
    common::adjust_quest_id,
    constants::{COMPLETED_LEVEQUEST_BITMASK_SIZE, COMPLETED_QUEST_BITMASK_SIZE},
    ipc::zone::{
        ActiveQuest, QuestActiveList, QuestTracker, ServerZoneIpcData, ServerZoneIpcSegment,
        TrackedQuest,
    },
};

impl ZoneConnection {
    fn active_quest_debug(&self) -> Vec<(u16, u8)> {
        self.player_data
            .quest
            .active
            .0
            .iter()
            .map(|quest| (quest.id, quest.sequence))
            .collect()
    }

    fn persist_quest_state(&mut self) {
    let mut database = self.database.lock();
    database.commit_player_data(&self.player_data);
    }


    async fn refresh_quest_views(&mut self) {
        self.send_active_quests().await;
        self.send_quest_tracker().await;
        self.send_scenario_guide().await;
        self.persist_quest_state();
    }

    pub async fn send_active_quests(&mut self) {
        let mut quests = Vec::new();
        for quest in &self.player_data.quest.active.0 {
            quests.push(ActiveQuest {
                id: quest.id,
                sequence: quest.sequence,
                flags: 1,
                ..Default::default()
            });
        }

        let ipc = ServerZoneIpcSegment::new(ServerZoneIpcData::QuestActiveList(QuestActiveList {
            quests,
        }));
        self.send_ipc_self(ipc).await;
    }

    pub async fn send_scenario_guide(&mut self) {
        let (quest_id_1, next_quest_id) = self
            .player_data
            .quest
            .active
            .0
            .first()
            .map(|quest| (quest.id as u32, quest.id as u32))
            .unwrap_or_default();

        let ipc = ServerZoneIpcSegment::new(ServerZoneIpcData::ScenarioGuide {
            quest_id_1,
            next_quest_id,
            layout_id: 0,
        });
        self.send_ipc_self(ipc).await;
    }

    pub async fn send_quest_information(&mut self) {
        self.send_active_quests().await;

        // quest complete list
        {
            let ipc = ServerZoneIpcSegment::new(ServerZoneIpcData::QuestCompleteList {
                completed_quests: self.player_data.quest.completed.data.clone(),
                unk2: vec![0xFF; 65],
            });
            self.send_ipc_self(ipc).await;
        }

        // legacy quest complete list
        {
            let ipc = ServerZoneIpcSegment::new(ServerZoneIpcData::LegacyQuestList {
                bitmask: [0xFF; 40],
            });
            self.send_ipc_self(ipc).await;
        }

        // levequest complete list
        // NOTE: all levequests are unlocked by default
        {
            let ipc = ServerZoneIpcSegment::new(ServerZoneIpcData::LevequestCompleteList {
                completed_levequests: vec![0xFF; COMPLETED_LEVEQUEST_BITMASK_SIZE],
                unk2: Vec::default(),
            });
            self.send_ipc_self(ipc).await;
        }

        self.send_quest_tracker().await;
        self.send_scenario_guide().await;
    }

    pub async fn accept_quest(&mut self, id: u32) {
        let adjusted_id = adjust_quest_id(id);
        tracing::info!(
            "Accepting quest raw_id={} adjusted_id={} active_before={:?}",
            id,
            adjusted_id,
            self.active_quest_debug()
        );
        if self.player_data.quest.completed.contains(adjusted_id) {
            tracing::warn!("Attempted to accept completed quest {adjusted_id}");
            return;
        }

        let index = if let Some(index) = self
            .player_data
            .quest
            .active
            .0
            .iter()
            .position(|quest| quest.id == adjusted_id as u16)
        {
            self.player_data.quest.active.0[index].sequence = 0xFF;
            index
        } else {
            self.player_data.quest.active.0.push(PersistentQuest {
                id: adjusted_id as u16,
                sequence: 0xFF,
            });
            self.player_data.quest.active.0.len() - 1
        };

        let ipc = ServerZoneIpcSegment::new(ServerZoneIpcData::AcceptQuest {
            quest_id: adjusted_id,
        });
        self.send_ipc_self(ipc).await;

        // Ensure its updated in the journal or whatever
        let ipc = ServerZoneIpcSegment::new(ServerZoneIpcData::UpdateQuest {
            index: index as u8,
            quest: ActiveQuest {
                id: adjusted_id as u16,
                sequence: 0xFF,
                flags: 1,
                ..Default::default()
            },
        });
        self.send_ipc_self(ipc).await;

        self.refresh_quest_views().await;
        tracing::info!(
            "Accepted quest adjusted_id={} active_after={:?}",
            adjusted_id,
            self.active_quest_debug()
        );
    }

    pub async fn finish_quest(&mut self, id: u32) {
        let adjusted_id = adjust_quest_id(id);
        tracing::info!(
            "Finishing quest raw_id={} adjusted_id={} active_before={:?}",
            id,
            adjusted_id,
            self.active_quest_debug()
        );

        // Remove it from our internal data model
        let index = if let Some(index) = self
            .player_data
            .quest
            .active
            .0
            .iter()
            .position(|x| x.id == adjusted_id as u16)
        {
            self.player_data.quest.active.0.remove(index);
            Some(index)
        } else {
            None
        };

        // Grant rewards
        let rewards;
        {
            let mut gamedata = self.gamedata.lock();
            rewards = gamedata.get_quest_rewards(id);
        }

        // Add gil
        // TODO: send log message
        self.player_data.inventory.currency.get_slot_mut(0).quantity += rewards.1;
        self.send_inventory().await;

        // Add exp
        self.add_exp(rewards.0 as i32).await;

        // Ensure its updated in the journal or whatever
        let ipc = ServerZoneIpcSegment::new(ServerZoneIpcData::UpdateQuest {
            index: index.unwrap_or_default() as u8,
            quest: ActiveQuest::default(),
        });
        self.send_ipc_self(ipc).await;

        self.player_data.quest.completed.set(adjusted_id);

        let ipc = ServerZoneIpcSegment::new(ServerZoneIpcData::FinishQuest {
            quest_id: adjusted_id as u16,
            flag1: 1,
            flag2: 1,
        });
        self.send_ipc_self(ipc).await;

        self.refresh_quest_views().await;
        tracing::info!(
            "Finished quest adjusted_id={} active_after={:?}",
            adjusted_id,
            self.active_quest_debug()
        );
    }

    pub async fn finish_all_quests(&mut self) {
        self.player_data.quest.completed.data = vec![0xFF; COMPLETED_QUEST_BITMASK_SIZE];
        self.persist_quest_state();
        self.send_quest_information().await;
    }

    pub async fn send_quest_tracker(&mut self) {
        // Right now we don't support tracking, so just send the first five quests.
        let mut tracked_quests = [TrackedQuest::default(); 5];
        for (i, _) in self.player_data.quest.active.0.iter().take(5).enumerate() {
            tracked_quests[i] = TrackedQuest {
                active: true,
                quest_index: i as u8,
            };
        }

        let ipc = ServerZoneIpcSegment::new(ServerZoneIpcData::QuestTracker(QuestTracker {
            tracked_quests,
        }));
        self.send_ipc_self(ipc).await;
    }

    pub async fn cancel_quest(&mut self, id: u32) {
        let adjusted_id = adjust_quest_id(id);

        // Remove it from our internal data model
        if let Some(index) = self
            .player_data
            .quest
            .active
            .0
            .iter()
            .position(|x| x.id == adjusted_id as u16)
        {
            self.player_data.quest.active.0.remove(index);
        }

        // TODO: inform the player, im not sure what this looks like in retail

        self.refresh_quest_views().await;
    }

    pub async fn set_quest_sequence(&mut self, id: u32, sequence: u8) {
        let adjusted_id = adjust_quest_id(id);
        tracing::info!(
            "Setting quest sequence raw_id={} adjusted_id={} sequence={} active_before={:?}",
            id,
            adjusted_id,
            sequence,
            self.active_quest_debug()
        );
        let Some((index, quest)) = self
            .player_data
            .quest
            .active
            .0
            .iter_mut()
            .enumerate()
            .find(|(_, quest)| quest.id == adjusted_id as u16)
        else {
            tracing::warn!("Attempted to set sequence for inactive quest {adjusted_id}");
            return;
        };

        quest.sequence = sequence;

        let ipc = ServerZoneIpcSegment::new(ServerZoneIpcData::UpdateQuest {
            index: index as u8,
            quest: ActiveQuest {
                id: adjusted_id as u16,
                sequence,
                flags: 1,
                ..Default::default()
            },
        });
        self.send_ipc_self(ipc).await;

        self.refresh_quest_views().await;
        tracing::info!(
            "Set quest sequence adjusted_id={} sequence={} active_after={:?}",
            adjusted_id,
            sequence,
            self.active_quest_debug()
        );
    }

    pub async fn incomplete_quest(&mut self, id: u32) {
        let adjusted_id = adjust_quest_id(id);
        self.player_data.quest.completed.clear(adjusted_id);
        self.persist_quest_state();
        self.send_quest_information().await;
    }

    pub async fn incomplete_all_quests(&mut self) {
        self.player_data.quest.completed.data = vec![0x0; COMPLETED_QUEST_BITMASK_SIZE];
        self.persist_quest_state();
        self.send_quest_information().await;
    }
}
