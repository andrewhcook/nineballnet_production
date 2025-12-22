use axum::{
    Json, Router, extract::{Path, Query, State, ws::{Message, WebSocket, WebSocketUpgrade}}, http::StatusCode, response::IntoResponse, routing::{any, get, post}
};
use futures_util::{StreamExt, SinkExt};
use tokio_tungstenite::{connect_async, tungstenite::protocol::Message as TungMessage};
use std::sync::{Arc, Mutex};

// ... (Keep existing AppState, ServerProcess structs) ...

#[tokio::main]
async fn main() {
    // ... (Log/Config setup) ...
 tracing_subscriber::fmt()
        .with_env_filter("allocator=debug,tower_http=debug")
        .init();

    // 2. Load Config
    let public_host = std::env::var("PUBLIC_HOST").unwrap_or_else(|_| "127.0.0.1".to_string());
    
    // Find where you define public_host and ensure it looks like this:
let state = Arc::new(AppState {
    active_servers: Mutex::new(HashMap::new()),
    // Render provides this system variable automatically
    public_host: std::env::var("RENDER_EXTERNAL_HOSTNAME").unwrap_or("localhost".into()), 
});

    let app = Router::new()
        .route("/allocate", post(allocate_server)) // Private: Called by Loco
        .route("/play/:match_id", any(proxy_handler)) // Public: Called by Players
        .with_state(state);

    // Render requires binding to 0.0.0.0
    // Change 3000 to 10000
let addr = std::net::SocketAddr::from(([0, 0, 0, 0], 10000));
println!("Allocator Proxy listening on {}", addr);
    
    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}

// --- THE NEW PROXY HANDLER ---
async fn proxy_handler(
    ws: WebSocketUpgrade,
    Path(match_id): Path<String>,
    Query(params): Query<HashMap<String, String>>,
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    // 1. Find the internal port for this match
    let target_port = {
        let servers = state.active_servers.lock().unwrap();
        // In a real app, you'd map match_id -> port efficiently. 
        // Here we scan for simplicity.
        servers.iter().find(|(_, p)| p.match_id == match_id).map(|(port, _)| *port)
    };

    let target_port = match target_port {
        Some(p) => p,
        None => return (StatusCode::NOT_FOUND, "Match not found").into_response(),
    };

    // 2. Extract Token to pass along
    let token_query = params.get("token").cloned().unwrap_or_default();

    // 3. Upgrade Client Connection to WebSocket
    ws.on_upgrade(move |client_socket| async move {
        handle_proxy(client_socket, target_port, token_query).await;
    })
}

async fn handle_proxy(mut client_socket: WebSocket, port: u16, token: String) {
    // 4. Connect internally to the Local Game Process
    // Note: The game server is running on localhost inside the same Render container
    let local_url = format!("ws://127.0.0.1:{}/?token={}", port, token);
    
    match connect_async(local_url).await {
        Ok((mut game_socket, _)) => {
            println!("Proxy established for port {}", port);
            
            // 5. Bridge the two streams (Client <-> Allocator <-> Game)
            let (mut client_sender, mut client_receiver) = client_socket.split();
            let (mut game_sender, mut game_receiver) = game_socket.split();

            let client_to_game = async {
                while let Some(Ok(msg)) = client_receiver.next().await {
                    let tungsten_msg = match msg {
                        Message::Text(t) => TungMessage::Text(t),
                        Message::Binary(b) => TungMessage::Binary(b),
                        Message::Close(_) => {
                            let _ = game_sender.close().await;
                            break;
                        },
                        _ => continue, // Ignore Pings for now
                    };
                    if game_sender.send(tungsten_msg).await.is_err() { break; }
                }
            };

            let game_to_client = async {
                while let Some(Ok(msg)) = game_receiver.next().await {
                    let axum_msg = match msg {
                        TungMessage::Text(t) => Message::Text(t),
                        TungMessage::Binary(b) => Message::Binary(b),
                        TungMessage::Close(_) => {
                             let _ = client_sender.send(Message::Close(None)).await;
                             break;
                        },
                        _ => continue,
                    };
                    if client_sender.send(axum_msg).await.is_err() { break; }
                }
            };

            // Run both directions until one fails
            tokio::select! {
                _ = client_to_game => {},
                _ = game_to_client => {},
            }
        }
        Err(e) => eprintln!("Failed to connect to local game server: {}", e),
    }
}



use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    process::{Command, Child},
    net::SocketAddr,
    time::{Duration, Instant},
};
use tracing::{info, error, warn};

// --- CONFIGURATION ---
const MIN_PORT: u16 = 8000;
const MAX_PORT: u16 = 9000; // Allows 1000 concurrent matches per node
const GAME_BINARY_PATH: &str = "./game_server"; // Ensure this matches your compiled binary location

// --- STATE MANAGEMENT ---
// We track the running process and when it started (for potential timeout logic)
struct ServerProcess {
    child: Child,
    started_at: Instant,
    match_id: String,
}

// Thread-safe state shared between the API and the Reaper
struct AppState {
    // Map Active Port -> Process Info
    active_servers: Mutex<HashMap<u16, ServerProcess>>,
    // Public IP or Hostname of this node (sent to clients)
    public_host: String,
}

// --- API DTOs ---
#[derive(Deserialize, Clone)]
struct AllocateRequest {
    match_id: String,
    p1_token: String,
    p2_token: String,
}

#[derive(Serialize)]
struct AllocateResponse {
    connect_url: String,
    port: u16,
    node_id: String,
}



// --- REAPER LOGIC ---
async fn run_reaper(state: Arc<AppState>) {
    let check_interval = Duration::from_secs(5);
    info!("Reaper task started. Checking for zombie processes every 5s.");

    loop {
        tokio::time::sleep(check_interval).await;
        
        let mut servers = state.active_servers.lock().unwrap();
        let mut ports_to_free = Vec::new();

        // Check every active server
        for (port, process) in servers.iter_mut() {
            // try_wait() returns Ok(Some(status)) if the process has exited
            match process.child.try_wait() {
                Ok(Some(status)) => {
                    info!(
                        "Reaping server for match {} on port {}. Exit status: {}", 
                        process.match_id, port, status
                    );
                    ports_to_free.push(*port);
                },
                Ok(None) => {
                    // Process is still running. 
                    // Optional: Check if it's been running too long (e.g., > 1 hour) and kill it?
                    // if process.started_at.elapsed() > Duration::from_secs(3600) { ... }
                },
                Err(e) => error!("Error checking process on port {}: {}", port, e),
            }
        }

        // Remove dead servers from the map to free up the ports
        for port in ports_to_free {
            servers.remove(&port);
        }
    }
}

// --- HANDLERS ---

async fn allocate_server(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<AllocateRequest>,
) -> impl IntoResponse {
    let mut servers = state.active_servers.lock().unwrap();

    // 1. Find a Free Port
    // We scan the range. In a massive system, you'd use a more efficient free-list.
    let port = (MIN_PORT..MAX_PORT)
        .find(|p| !servers.contains_key(p));

    let port = match port {
        Some(p) => p,
        None => {
            error!("Allocation failed: No ports available!");
            return (StatusCode::SERVICE_UNAVAILABLE, "No ports available").into_response();
        }
    };

    info!("Spawning match {} on port {}", payload.match_id, port);

    // 2. Spawn the Game Binary
    // NOTE: In production, ensure 'game_server' is in the working directory or PATH
    let spawn_result = Command::new(GAME_BINARY_PATH)
        .args(&[
            "--port", &port.to_string(),
            "--p1-token", &payload.p1_token,
            "--p2-token", &payload.p2_token,
            "--match-id", &payload.match_id,
        ])
        // Inherit logs so we can see game server output in the Allocator's console
        .stdout(std::process::Stdio::inherit())
        .stderr(std::process::Stdio::inherit())
        .spawn();

    match spawn_result {
        Ok(child) => {
            // 3. Track the Process
            servers.insert(port, ServerProcess {
                child,
                started_at: Instant::now(),
                match_id: payload.clone().match_id.clone(),
            });

            // 4. Return Connection Info
            // Returns: ws://203.0.113.45:8001
           let connect_url = format!("wss://{}/play/{}", state.public_host, payload.match_id);

           (StatusCode::OK, Json(AllocateResponse {
            connect_url,
            port,
            node_id: "allocator-01".to_string(),
        })).into_response()
        },
        Err(e) => {
            error!("Failed to spawn game binary: {}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, "Failed to spawn process").into_response()
        }
    }
}

async fn health_check() -> impl IntoResponse {
    Json(serde_json::json!({ "status": "ok", "role": "bastion" }))
}