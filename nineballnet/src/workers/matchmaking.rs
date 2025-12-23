use serde::{Deserialize, Serialize};
use loco_rs::prelude::*;
use crate::models::_entities::matches;
use sea_orm::{ActiveValue::Set, ActiveModelTrait};
// CRITICAL IMPORT: Needed for deleting old records
use sea_orm::{ColumnTrait, QueryFilter, EntityTrait}; 
use async_trait::async_trait;
use chrono::Utc;
use uuid::Uuid;
use redis::AsyncCommands;

#[derive(Deserialize, Debug)]
struct AllocationResponse {
    connect_url: String,
    port: u16,
    node_id: String,
}

pub struct MatchmakingWorker {
    pub ctx: AppContext,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct MatchmakingWorkerArgs {
    pub player_id: Uuid,
}

#[async_trait]
impl BackgroundWorker<MatchmakingWorkerArgs> for MatchmakingWorker {
    fn build(ctx: &AppContext) -> Self {
        Self { ctx: ctx.clone() }
    }

    async fn perform(&self, args: MatchmakingWorkerArgs) -> Result<()> {
        let queue_key = "matchmaking_queue";
        
        // 1. Connect to Redis
        let redis_url = std::env::var("REDIS_URL")
            .unwrap_or_else(|_| "redis://127.0.0.1:6379".to_string());

        let client = redis::Client::open(redis_url)
            .map_err(|e| Error::Message(format!("Invalid Redis URL: {}", e)))?;

        let mut redis = client.get_multiplexed_async_connection().await
            .map_err(|e| Error::Message(format!("Redis Connection Failed: {}", e)))?;

        // 2. Queue Logic
        let _: () = redis.rpush(queue_key, args.player_id.to_string()).await
            .map_err(|e| Error::Message(format!("Redis Push Error: {}", e)))?;
        
        println!("WORKER: Player {} added to queue.", args.player_id);

        let queue_len: isize = redis.llen(queue_key).await
            .map_err(|e| Error::Message(e.to_string()))?;

        if queue_len < 2 {
            println!("WORKER: Waiting for opponent (Queue: {})", queue_len);
            return Ok(());
        }

        // Pop 2 Players
        let p1_str: Option<String> = redis.lpop(queue_key, None).await.ok();
        let p2_str: Option<String> = redis.lpop(queue_key, None).await.ok();

        let (p1_id, p2_id) = match (p1_str, p2_str) {
            (Some(p1), Some(p2)) => (p1, p2),
            (Some(p1), None) => {
                let _: () = redis.rpush(queue_key, p1).await.ok().unwrap();
                return Ok(());
            },
            _ => return Ok(()),
        };

        if p1_id == p2_id {
             return Ok(());
        }

        println!("WORKER: MATCH FOUND! {} vs {}", p1_id, p2_id);

        // --- CRITICAL FIX: CLEANUP OLD MATCHES ---
        // This deletes any previous "Ready" records for these players.
        // Without this, the frontend grabs the OLD match (Port 8000) instead of the new one.
        matches::Entity::delete_many()
            .filter(matches::Column::PlayerId.eq(Uuid::parse_str(&p1_id).unwrap()))
            .exec(&self.ctx.db)
            .await
            .map_err(|e| Error::Message(format!("DB Cleanup P1 Failed: {}", e)))?;

        matches::Entity::delete_many()
            .filter(matches::Column::PlayerId.eq(Uuid::parse_str(&p2_id).unwrap()))
            .exec(&self.ctx.db)
            .await
            .map_err(|e| Error::Message(format!("DB Cleanup P2 Failed: {}", e)))?;


        // 3. Allocator Logic
        let match_uuid = Uuid::new_v4();
        // Generate NEW random tokens for this specific match
        let p1_token = Uuid::new_v4().to_string(); 
        let p2_token = Uuid::new_v4().to_string(); 

        let allocator_url = std::env::var("ALLOCATOR_URL")
            .unwrap_or_else(|_| "http://localhost:3000".to_string());

        let http_client = reqwest::Client::new();
        let response = http_client
            .post(format!("{}/allocate", allocator_url))
            .json(&serde_json::json!({
                "match_id": match_uuid.to_string(),
                "p1_token": p1_token,
                "p2_token": p2_token
            }))
            .send()
            .await
            .map_err(|e| Error::Message(e.to_string()))?;

        if !response.status().is_success() {
            return Err(Error::Message("Allocator failed".into()));
        }

        let allocation: AllocationResponse = response.json().await
            .map_err(|e| Error::Message(e.to_string()))?;
        
        println!("WORKER: Server allocated at {}", allocation.connect_url);

        // 4. DB Updates (Insert NEW match)
        let now = Utc::now().naive_utc();

        let record_p1 = matches::ActiveModel {
            match_id: Set(match_uuid),
            player_id: Set(Uuid::parse_str(&p1_id).unwrap()),
            status: Set("ready".to_string()),
            gateway_url: Set(Some(allocation.connect_url.clone())), 
            handoff_token: Set(Some(p1_token)), 
            created_at: Set(now),
            updated_at: Set(now),
            ..Default::default()
        };

        let record_p2 = matches::ActiveModel {
            match_id: Set(match_uuid),
            player_id: Set(Uuid::parse_str(&p2_id).unwrap()),
            status: Set("ready".to_string()),
            gateway_url: Set(Some(allocation.connect_url)), 
            handoff_token: Set(Some(p2_token)), 
            created_at: Set(now),
            updated_at: Set(now),
            ..Default::default()
        };

        record_p1.insert(&self.ctx.db).await.map_err(|e| Error::Message(e.to_string()))?;
        record_p2.insert(&self.ctx.db).await.map_err(|e| Error::Message(e.to_string()))?;

        println!("WORKER: DB updated.");
        Ok(())
    }
}