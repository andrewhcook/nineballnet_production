use serde::{Deserialize, Serialize};
use loco_rs::prelude::*;
use crate::models::_entities::matches;
use sea_orm::{ActiveValue::Set, ActiveModelTrait};
use async_trait::async_trait;
use chrono::Utc;
use uuid::Uuid;
use redis::AsyncCommands; // Enables .rpush / .lpop

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
        
        // --- 1. CONNECT TO REDIS MANUALLY ---
        let redis_url = std::env::var("REDIS_URL")
            .unwrap_or_else(|_| "redis://127.0.0.1:6379".to_string());

        let client = redis::Client::open(redis_url)
            .map_err(|e| Error::Message(format!("Invalid Redis URL: {}", e)))?;

        // FIX: Use 'get_multiplexed_async_connection' which is the standard async method
        let mut redis = client.get_multiplexed_async_connection().await
            .map_err(|e| Error::Message(format!("Redis Connection Failed: {}", e)))?;

        // --- 2. QUEUE LOGIC ---

        // Push current player
        let _: () = redis.rpush(queue_key, args.player_id.to_string()).await
            .map_err(|e| Error::Message(format!("Redis Push Error: {}", e)))?;
        
        println!("WORKER: Player {} added to queue.", args.player_id);

        // Check queue length
        let queue_len: isize = redis.llen(queue_key).await
            .map_err(|e| Error::Message(e.to_string()))?;

        if queue_len < 2 {
            println!("WORKER: Queue length is {}. Waiting for more players.", queue_len);
            return Ok(());
        }

        // Pop 2 Players
        // NOTE: If your specific redis version errors on "None", remove the ", None" argument.
        // Standard modern redis crate expects lpop(key, count).
        let player_1_str: Option<String> = redis.lpop(queue_key, None).await.ok();
        let player_2_str: Option<String> = redis.lpop(queue_key, None).await.ok();

        // Safety Check
        let (p1_id, p2_id) = match (player_1_str, player_2_str) {
            (Some(p1), Some(p2)) => (p1, p2),
            (Some(p1), None) => {
                // Return the straggler to the queue
                let _: () = redis.rpush(queue_key, p1).await.ok().unwrap();
                return Ok(());
            },
            _ => return Ok(()),
        };

        if p1_id == p2_id {
             println!("WORKER: Duplicate player found. Ignoring.");
             return Ok(());
        }

        println!("WORKER: MATCH FOUND! {} vs {}", p1_id, p2_id);

        // --- 3. ALLOCATION LOGIC ---

        let match_uuid = Uuid::new_v4();
        let p1_token = format!("token_{}_p1", match_uuid); 
        let p2_token = format!("token_{}_p2", match_uuid); 

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

        // --- 4. DB UPDATES ---
        
        let now = Utc::now().naive_utc();

        // Record for Player 1
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

        // Record for Player 2
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

        println!("WORKER: DB updated for both players.");
        Ok(())
    }
}