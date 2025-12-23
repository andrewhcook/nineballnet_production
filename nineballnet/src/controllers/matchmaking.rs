use loco_rs::prelude::*;
use crate::{
    workers::matchmaking::{MatchmakingWorker, MatchmakingWorkerArgs},
    models::_entities::matches,
};
use sea_orm::{ColumnTrait, QueryFilter, EntityTrait, QueryOrder};
use loco_rs::bgworker::BackgroundWorker; 
use uuid::Uuid; // Ensure this import is present

// POST /api/matchmaking/find
pub async fn find(
    auth: auth::JWT,
    State(ctx): State<AppContext>,
) -> Result<Response> {
    // 1. Convert the String PID from JWT into a Uuid
    let player_id = Uuid::parse_str(&auth.claims.pid).map_err(|_| {
        Error::BadRequest("Invalid player ID format in token".to_string())
    })?;

    matches::Entity::delete_many()
        .filter(matches::Column::PlayerId.eq(player_id))
        .exec(&ctx.db)
        .await?;

    // 2. Enqueue the job
    MatchmakingWorker::perform_later(
        &ctx, 
        MatchmakingWorkerArgs { player_id }
    ).await?;

    format::json("Search started")
}

// GET /api/matchmaking/status
pub async fn status(
    auth: auth::JWT,
    State(ctx): State<AppContext>,
) -> Result<Response> {
    // 1. Convert the String PID from JWT into a Uuid
    let player_id = Uuid::parse_str(&auth.claims.pid).map_err(|_| {
        Error::BadRequest("Invalid player ID format in token".to_string())
    })?;

    // 2. Check DB for the ticket using the parsed Uuid
    let ticket = matches::Entity::find()
        .filter(matches::Column::PlayerId.eq(player_id)) // Now types match (Uuid vs Uuid)
        .filter(matches::Column::Status.eq("ready"))
        .order_by_desc(matches::Column::CreatedAt)
        .one(&ctx.db)
        .await?;

    match ticket {
        Some(t) => format::json(t),
        None => format::text("searching"), 
    }
}

pub fn routes() -> Routes {
    Routes::new()
        .prefix("api/matchmaking")
        .add("/find", post(find))
        .add("/status", get(status))
}