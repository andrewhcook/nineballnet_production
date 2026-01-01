use loco_rs::prelude::*;
use crate::{
    workers::matchmaking::{MatchmakingWorker, MatchmakingWorkerArgs},
    models::_entities::{matches, users}, // Added users for the cookie lookup
    models::matches::Entity as Matches,  // Alias for clearer queries
    models::users::Entity as Users,      // Added for join auth
};
use sea_orm::{
    ColumnTrait, QueryFilter, EntityTrait, QueryOrder, 
    ActiveModelTrait, ActiveValue::Set, TransactionTrait, Condition
};
use loco_rs::bgworker::BackgroundWorker; 
use uuid::Uuid;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use axum_extra::extract::cookie::CookieJar; // Needed for Join

// --- DTOs ---

#[derive(Deserialize)]
pub struct JoinGameParams {
    pub match_id: String,
}

#[derive(Serialize)]
pub struct JoinGameResponse {
    pub gateway_url: String,
    pub handoff_token: String,
}

#[derive(Deserialize)]
struct AllocatorResponse {
    connect_url: String,
}

// --- HANDLERS ---

// POST /api/matchmaking/find
pub async fn find(
    auth: auth::JWT,
    State(ctx): State<AppContext>,
) -> Result<Response> {
    let player_id = Uuid::parse_str(&auth.claims.pid).map_err(|_| {
        Error::BadRequest("Invalid player ID".to_string())
    })?;

    // 1. FORCE CLEANUP: Mark ANY active/zombie games as 'abandoned'
    // This acts as a self-healing mechanism. If they were stuck in a loop,
    // clicking "Find Match" breaks the loop.
    matches::Entity::update_many()
        .col_expr(matches::Column::Status, sea_orm::sea_query::Expr::value("abandoned"))
        .filter(matches::Column::PlayerId.eq(player_id))
        .filter(
            Condition::any()
                .add(matches::Column::Status.eq("ready"))
                .add(matches::Column::Status.eq("searching"))
                .add(matches::Column::Status.eq("open"))
        )
        .exec(&ctx.db)
        .await
        .map_err(|e| Error::DB(e))?;

    // 2. Start the New Search
    MatchmakingWorker::perform_later(
        &ctx, 
        MatchmakingWorkerArgs { player_id }
    ).await?;

    format::json("Search started")
}

pub async fn status(
    auth: auth::JWT,
    State(ctx): State<AppContext>,
) -> Result<Response> {
    let player_id = Uuid::parse_str(&auth.claims.pid).map_err(|_| {
        Error::BadRequest("Invalid player ID".to_string())
    })?;

    // Find the most recent active interaction for this player
    let active_game = Matches::find()
        .filter(matches::Column::PlayerId.eq(player_id))
        // Look for ANY active state
        .filter(
            Condition::any()
                .add(matches::Column::Status.eq("ready"))
                .add(matches::Column::Status.eq("searching"))
                .add(matches::Column::Status.eq("open"))
        )
        .order_by_desc(matches::Column::CreatedAt)
        .one(&ctx.db)
        .await
        .map_err(|e| Error::DB(e))?;

    match active_game {
        // CASE 1: Game is Ready -> Send Ticket
        Some(game) if game.status == "ready" => format::json(game),
        
        // CASE 2: Still Waiting -> Tell frontend to keep polling
        Some(_) => format::text("searching"),
        
        // CASE 3: No active game -> Tell frontend to sit idle
        None => format::text("idle"), 
    }
}

// POST /api/matchmaking/join
pub async fn join(
    auth: auth::JWT, // <--- Switch to this extractor
    State(ctx): State<AppContext>,
    Json(params): Json<JoinGameParams>,
) -> Result<Response> {
    // 1. Authenticate (Handled by auth::JWT)
    // We extract the PID directly from the validated token claims
    let joiner_pid = Uuid::parse_str(&auth.claims.pid).map_err(|_| {
        Error::BadRequest("Invalid player ID in token".to_string())
    })?;

    let target_match_id = Uuid::parse_str(&params.match_id).map_err(|_| {
        Error::BadRequest("Invalid match ID".to_string())
    })?;

    // 2. Transaction Start
    let txn = ctx.db.begin().await.map_err(|e| Error::DB(e))?;

    // 3. Find Host's Game
    let host_match = Matches::find()
        .filter(matches::Column::MatchId.eq(target_match_id))
        .filter(
            Condition::any()
                .add(matches::Column::Status.eq("searching"))
                .add(matches::Column::Status.eq("open"))
        )
        .one(&txn)
        .await
        .map_err(|e| Error::DB(e))?
        .ok_or_else(|| Error::NotFound)?;

    // Guard: Prevent joining your own game
    if host_match.player_id == joiner_pid {
        return Err(Error::BadRequest("Cannot join your own game".into()));
    }

    // 4. Allocate Server
    let p1_token = Uuid::new_v4().to_string();
    let p2_token = Uuid::new_v4().to_string();

    let allocator_url = std::env::var("ALLOCATOR_URL").unwrap_or_else(|_| "http://localhost:3000".to_string());
    let client = reqwest::Client::new();
    
    let alloc_res = client.post(format!("{}/allocate", allocator_url))
        .json(&serde_json::json!({
            "match_id": host_match.match_id.to_string(),
            "p1_token": p1_token,
            "p2_token": p2_token
        }))
        .send()
        .await
        .map_err(|_| Error::InternalServerError)?;

    if !alloc_res.status().is_success() {
        return Err(Error::InternalServerError);
    }
    let alloc_data: AllocatorResponse = alloc_res.json().await.map_err(|_| Error::InternalServerError)?;

    // 5. Update DB Rows
    let now = Utc::now().naive_utc();

    // Update Host (P1)
    let mut host_active: matches::ActiveModel = host_match.into();
    host_active.status = Set("ready".to_string());
    host_active.gateway_url = Set(Some(alloc_data.connect_url.clone()));
    host_active.handoff_token = Set(Some(p1_token));
    host_active.updated_at = Set(now);
    host_active.save(&txn).await.map_err(|e| Error::DB(e))?;

    // Insert Joiner (P2)
    let joiner_row = matches::ActiveModel {
        match_id: Set(target_match_id),
        player_id: Set(joiner_pid),
        status: Set("ready".to_string()),
        gateway_url: Set(Some(alloc_data.connect_url.clone())),
        handoff_token: Set(Some(p2_token.clone())),
        created_at: Set(now),
        updated_at: Set(now),
        ..Default::default()
    };
    joiner_row.insert(&txn).await.map_err(|e| Error::DB(e))?;

    txn.commit().await.map_err(|e| Error::DB(e))?;

    // 6. Return Connection Info
    format::json(JoinGameResponse {
        gateway_url: alloc_data.connect_url,
        handoff_token: p2_token,
    })
}



// POST /api/matchmaking/leave
pub async fn leave(
    auth: auth::JWT,
    State(ctx): State<AppContext>,
) -> Result<Response> {
    let player_id = Uuid::parse_str(&auth.claims.pid).map_err(|_| {
        Error::BadRequest("Invalid player ID".to_string())
    })?;

    // 1. Find active games for this player
    let games = Matches::find()
        .filter(matches::Column::PlayerId.eq(player_id))
        .filter(matches::Column::Status.ne("finished"))
        .filter(matches::Column::Status.ne("abandoned"))
        .all(&ctx.db)
        .await
        .map_err(|e| Error::DB(e))?;

    if games.is_empty() {
        return format::text("No active games");
    }

    let allocator_url = std::env::var("ALLOCATOR_URL")
        .unwrap_or_else(|_| "http://localhost:10000".to_string());
    
    let client = reqwest::Client::new();

    for game in games {
        // --- SCENARIO A: GAME WAS ACTIVE (Rage Quit) ---
        if game.status == "ready" {
            // 1. Notify Allocator to kill process
            if let Some(token) = game.handoff_token.clone() {
                let match_id = game.match_id.to_string();
                let url = allocator_url.clone();
                let client_clone = client.clone();

                tokio::spawn(async move {
                    let _ = client_clone.post(format!("{}/deallocate", url))
                        .json(&serde_json::json!({ 
                            "match_id": match_id, 
                            "token": token 
                        }))
                        .send()
                        .await;
                });
            }

            // 2. Mark EVERYONE in the match as 'finished'
            // This triggers the "Opponent Disconnected" watchdog for the other player
            matches::Entity::update_many()
                .col_expr(matches::Column::Status, sea_orm::sea_query::Expr::value("finished"))
                .filter(matches::Column::MatchId.eq(game.match_id))
                .exec(&ctx.db)
                .await
                .map_err(|e| Error::DB(e))?;
        } 
        
        // --- SCENARIO B: CANCELLING SEARCH (Lobby/Queue) ---
        else {
            // The game never started. Just remove it from the Lobby/Queue.
            // We mark it 'abandoned' so it stops showing up in the "Browse" list.
            let mut active: matches::ActiveModel = game.into();
            active.status = Set("abandoned".to_string());
            active.updated_at = Set(Utc::now().naive_utc());
            active.save(&ctx.db).await.map_err(|e| Error::DB(e))?;
        }
    }

    format::text("Left game/queue")
}


// GET /api/matchmaking/list
pub async fn list(State(ctx): State<AppContext>) -> Result<Response> {
    // 1. Join Matches with Users to get Host Name/Rating
    // We want games that are "searching" (waiting for an opponent)
    let games = Matches::find()
        .filter(matches::Column::Status.eq("searching")) 
        .find_also_related(users::Entity)
        .order_by_desc(matches::Column::CreatedAt)
        .all(&ctx.db)
        .await
        .map_err(|e| Error::DB(e))?;

    // 2. Map to a clean JSON format for the frontend
    let response_data: Vec<serde_json::Value> = games.into_iter().map(|(game, user)| {
        let user = user.unwrap(); // Safety: Match must have a player
        
        // This JSON structure matches exactly what your JS needs
        serde_json::json!({
            "id": game.match_id,
            "host_name": user.name,
            "host_rating": "1500?", // Default 1200 if None
            "config_description": "Nine-Ball", // Hardcoded for now, or fetch from game config
            "is_rated": true, // Hardcoded or fetch from game config
            "created_at": game.created_at
        })
    }).collect();

    format::json(response_data)
}

pub fn routes() -> Routes {
    Routes::new()
        .prefix("api/matchmaking")
        .add("/find", post(find))
        .add("/status", get(status))
        .add("/join", post(join))
        .add("/leave", post(leave))
        .add("/list", get(list))
}