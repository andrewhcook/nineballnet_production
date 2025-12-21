use serde::{Deserialize, Serialize};
use loco_rs::prelude::*;
use crate::models::_entities::matches;
use sea_orm::{ActiveValue::Set, ActiveModelTrait};
use async_trait::async_trait;
use chrono::Utc;
use uuid::Uuid;

// 1. Add a struct to capture the Allocator's response
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
        println!("WORKER: Matchmaking started for player {}", args.player_id);

        // 2. Generate Match ID and Secure Tokens
        let match_uuid = Uuid::new_v4();
        let p1_token = format!("token_{}_p1", match_uuid); // Secure random string in prod
        let p2_token = format!("token_{}_p2", match_uuid); // Placeholder for 2nd player

        // 3. Define the Allocator Internal URL
        // On Render, this is usually http://service-name:port
        let allocator_url = std::env::var("ALLOCATOR_URL")
            .unwrap_or_else(|_| "http://localhost:3000".to_string());

        println!("WORKER: Requesting server from Allocator at {}", allocator_url);

        // 4. Call the Allocator Service
        let client = reqwest::Client::new();
        let response = client
            .post(format!("{}/allocate", allocator_url))
            .json(&serde_json::json!({
                "match_id": match_uuid.to_string(),
                "p1_token": p1_token,
                "p2_token": p2_token
            }))
            .send()
            .await
            .map_err(|e| {
                tracing::error!("Failed to contact Allocator: {}", e);
                Error::msg(e)
            })?;

        if !response.status().is_success() {
            let status = response.status();
            tracing::error!("Allocator returned error: {}", status);
            return Err(Error::Message(format!("Allocator failed: {}", status)));
        }

        // 5. Parse the Proxy URL
        // The Allocator returns something like: "wss://allocator-srv.onrender.com/play/UUID"
        let allocation: AllocationResponse = response.json().await.map_err(|e| {
             tracing::error!("Failed to parse Allocator response: {}", e);
             Error::msg(e)
        })?;

        println!("WORKER: Allocated server at {}", allocation.connect_url);

        // 6. Write the Match Record to Postgres
        let now = Utc::now().naive_utc();
        
        let match_record = matches::ActiveModel {
            match_id: Set(match_uuid),
            player_id: Set(args.player_id),
            status: Set("ready".to_string()),
            // IMPORTANT: Save the Proxy URL, not the local IP!
            gateway_url: Set(Some(allocation.connect_url)), 
            handoff_token: Set(Some(p1_token)), 
            created_at: Set(now),
            updated_at: Set(now),
            ..Default::default()
        };

        match_record.insert(&self.ctx.db).await.map_err(|e| {
            tracing::error!("DB Error: {}", e);
            Error::msg(e)
        })?;

        println!("WORKER: Match {} ready!", match_uuid);
        Ok(())
    }
}