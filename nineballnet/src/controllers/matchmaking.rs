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

// POST /api/matchmaking/find (Unchanged)
pub async fn find(
    auth: auth::JWT,
    State(ctx): State<AppContext>,
) -> Result<Response> {
    let player_id = Uuid::parse_str(&auth.claims.pid).map_err(|_| {
        Error::BadRequest("Invalid player ID format in token".to_string())
    })?;

    matches::Entity::delete_many()
        .filter(matches::Column::PlayerId.eq(player_id))
        .exec(&ctx.db)
        .await?;

    MatchmakingWorker::perform_later(
        &ctx, 
        MatchmakingWorkerArgs { player_id }
    ).await?;

    format::json("Search started")
}

// GET /api/matchmaking/status (Unchanged)
pub async fn status(
    auth: auth::JWT,
    State(ctx): State<AppContext>,
) -> Result<Response> {
    let player_id = Uuid::parse_str(&auth.claims.pid).map_err(|_| {
        Error::BadRequest("Invalid player ID format in token".to_string())
    })?;

    let ticket = matches::Entity::find()
        .filter(matches::Column::PlayerId.eq(player_id))
        .filter(matches::Column::Status.eq("ready"))
        .order_by_desc(matches::Column::CreatedAt)
        .one(&ctx.db)
        .await?;

    match ticket {
        Some(t) => format::json(t),
        None => format::text("searching"), 
    }
}

// POST /api/matchmaking/join
pub async fn join(
    State(ctx): State<AppContext>,
    jar: CookieJar,
    Json(params): Json<JoinGameParams>,
) -> Result<Response> {
    // 1. Authenticate
    let token = jar.get("token").map(|c| c.value()).ok_or_else(|| Error::Unauthorized("No token".into()))?;
    
    // We use 'ApiKey' because that is the default column name for tokens in Loco
    let joiner = Users::find()
        .filter(users::Column::ApiKey.eq(token))
        .one(&ctx.db)
        .await
        .map_err(|e| Error::DB(e))? 
        .ok_or_else(|| Error::Unauthorized("Invalid token".into()))?;

    let target_match_id = Uuid::parse_str(&params.match_id).map_err(|_| {
        Error::BadRequest("Invalid match ID".to_string())
    })?;

    // 2. Transaction Start
    // CRITICAL FIX: Use closure |e| Error::DB(e) here
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
    
    if host_match.player_id == joiner.pid {
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
        player_id: Set(joiner.pid),
        status: Set("ready".to_string()),
        gateway_url: Set(Some(alloc_data.connect_url.clone())),
        handoff_token: Set(Some(p2_token.clone())),
        created_at: Set(now),
        updated_at: Set(now),
        ..Default::default()
    };
    joiner_row.insert(&txn).await.map_err(|e| Error::DB(e))?;

    // Commit Transaction
    // CRITICAL FIX: explicit closure here too
    txn.commit().await.map_err(|e| Error::DB(e))?;

    // 6. Return Connection Info
    format::json(JoinGameResponse {
        gateway_url: alloc_data.connect_url,
        handoff_token: p2_token,
    })
}

pub fn routes() -> Routes {
    Routes::new()
        .prefix("api/matchmaking")
        .add("/find", post(find))
        .add("/status", get(status))
        .add("/join", post(join))
}