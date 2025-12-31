use loco_rs::prelude::*;
use axum::response::Redirect;
use axum_extra::extract::cookie::CookieJar;
use serde_json::json;
use loco_rs::controller::views::engines::TeraView;

// Import DB entities (Only needed for the 'browse' view now)
use crate::models::{
    _entities::{matches, users}, 
    matches::Entity as Matches, 
    users::Entity as Users
};
use sea_orm::{
    EntityTrait, QueryFilter, QueryOrder, QuerySelect, ColumnTrait
};

#[derive(serde::Serialize)]
pub struct LobbyGameView {
    pub id: String,
    pub host_name: String,
    pub host_rating: i32,
    pub rating_provisional: bool,
    pub config_description: String,
    pub is_rated: bool,
    pub status: String,
}

// --- HANDLERS ---

// 1. DEFAULT VIEW: Game Guide & "Find Match" (GET /lobby)
pub async fn index(
    ViewEngine(v): ViewEngine<TeraView>,
    jar: CookieJar,
) -> Result<Response> {
    // Check Auth
    let raw_token = match jar.get("token") {
        Some(c) => c.value().to_string(),
        None => return Ok(Redirect::to("/auth/login").into_response()),
    };

    // Render the "Simple" view by default
    // Ensure your matchmaking HTML is saved as "home/lobby_matchmaking.html" (or similar)
    format::render().view(
        &v,
        "home/matchmaking.html", 
        json!({
            "token": raw_token
        })
    )
}

// 2. DETAILED VIEW: Server Browser Table (GET /lobby/browse)
pub async fn browse(
    ViewEngine(v): ViewEngine<TeraView>,
    State(ctx): State<AppContext>,
    jar: CookieJar,
) -> Result<Response> {
    let raw_token = match jar.get("token") {
        Some(c) => c.value().to_string(),
        None => return Ok(Redirect::to("/auth/login").into_response()),
    };

    // Fetch Games List
    let active_requests: Vec<(matches::Model, Option<users::Model>)> = Matches::find()
        .filter(matches::Column::Status.eq("searching")) 
        .order_by_desc(matches::Column::CreatedAt)
        .find_also_related(Users)
        .all(&ctx.db)
        .await?;

    let games: Vec<LobbyGameView> = active_requests
        .into_iter()
        .map(|(match_req, user)| {
            let host_name = user.map(|u| u.name).unwrap_or_else(|| "Unknown".to_string());
            LobbyGameView {
                id: match_req.match_id.to_string(),
                host_name,
                host_rating: 1500, 
                rating_provisional: true, 
                config_description: "Standard 9-Ball".to_string(),
                is_rated: true,
                status: match_req.status,
            }
        })
        .collect();

    // Render the "Table" view
    format::render().view(
        &v, 
        "home/lobby_browse.html", // Rename your old table view to this!
        json!({
            "games": games,
            "token": raw_token,
        })
    )
}

pub fn routes() -> Routes {
    Routes::new()
        .prefix("lobby")
        .add("/", get(index))      // -> Landing Page (Guide)
        .add("/browse", get(browse)) // -> Table View
}