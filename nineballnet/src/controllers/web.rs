use loco_rs::prelude::*;
use axum::response::{IntoResponse, Redirect};
use axum_extra::extract::cookie::CookieJar;
use serde_json::json;

pub async fn index(
    jar: CookieJar, 
    ViewEngine(v): ViewEngine<TeraView>
) -> Result<impl IntoResponse> {
    // 1. Check if the "token" cookie exists
    if jar.get("token").is_some() {
        // User is logged in, send them to the game lobby
        return Ok(Redirect::to("/lobby").into_response());
    }

    // 2. User is NOT logged in, render the Login View directly at "/"
    format::render().view(&v, "auth/login.html", json!({}))
}

pub fn routes() -> Routes {
    Routes::new()
        .add("/", get(index))
}