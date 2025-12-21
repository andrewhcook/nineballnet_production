use loco_rs::prelude::*;
use axum::http::StatusCode;
use loco_rs::controller::views::engine::TeraView;
use serde_json::json;
use axum_extra::extract::cookie::CookieJar;

pub async fn index(
    v: ViewEngine<TeraView>,
    jar: CookieJar,           // 2. Extract the cookies
    auth: auth::JWT,          // 3. Keep your existing auth logic
) -> Result<Response> {
    // 4. Pull the "token" string directly from the jar
    // This is the same key you defined in your YAML: location: { Cookie: "token" }
    let raw_token = jar
        .get("token")
        .map(|c| c.value())
        .unwrap_or("");

    format::render()
        .view(
            &v.0, 
            "home/lobby.html", 
            json!({
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