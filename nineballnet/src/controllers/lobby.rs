use loco_rs::prelude::*;
use axum_extra::extract::cookie::CookieJar;
use serde::{Serialize, Deserialize};
use serde_json::json;
use loco_rs::controller::views::engines::TeraView;

// Import the database entities so we can query matches
use crate::models::{
    _entities::{matches, users}, 
    matches::Entity as Matches, 
    users::Entity as Users
};
use sea_orm::{
    EntityTrait, QueryFilter, QueryOrder, QuerySelect, ColumnTrait
};

#[derive(Serialize)]
pub struct LobbyGameView {
    pub id: String,
    pub host_name: String,
    pub host_rating: i32,
    pub rating_provisional: bool,
    pub config_description: String,
    pub is_rated: bool,
    pub status: String,
}

pub async fn index(
    ViewEngine(v): ViewEngine<TeraView>,
    State(ctx): State<AppContext>, // Added: Needed for DB access
    jar: CookieJar,
    auth: auth::JWT,
) -> Result<Response> {
    // 1. DATABASE LOGIC: Fetch the list of open games
    // ------------------------------------------------
    let active_requests: Vec<(matches::Model, Option<users::Model>)> = Matches::find()
        .filter(matches::Column::Status.eq("searching")) 
        .order_by_desc(matches::Column::CreatedAt)
        .find_also_related(Users)
        .all(&ctx.db)
        .await?;

    // Map DB results to the View Struct
    let games: Vec<LobbyGameView> = active_requests
        .into_iter()
        .map(|(match_req, user)| {
            let host_name = user.map(|u| u.name).unwrap_or_else(|| "Unknown".to_string());
            
            LobbyGameView {
                id: match_req.match_id.to_string(),
                host_name,
                host_rating: 1500, // Placeholder until you add rating to Users
                rating_provisional: true, 
                config_description: "Standard 9-Ball".to_string(),
                is_rated: true,
                status: match_req.status,
            }
        })
        .collect();

    // 2. AUTH LOGIC: Get the token for the frontend JS
    // ------------------------------------------------
    let raw_token = jar
        .get("token")
        .map(|c| c.value())
        .unwrap_or("");

    // 3. RENDER
    // ------------------------------------------------
    // We pass 'games' for the table, and 'token' for the JS buttons
    format::render().view(
        &v, 
        "home/lobby_visualized.html", // Ensure this path matches where you saved the HTML file!
        json!({
            "games": games,
            "token": raw_token,
            "player_id": auth.claims.pid
        })
    )
}

pub fn routes() -> Routes {
    Routes::new()
        .prefix("lobby")
        .add("/", get(index))
}