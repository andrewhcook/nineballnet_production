use axum::{
    extract::{Path, Query, State, ws::{Message, WebSocket, WebSocketUpgrade}},
    http::StatusCode,
    response::IntoResponse,
    routing::{any, post}, // Added 'post' explicitly
    Json, Router,
};
use futures_util::{StreamExt, SinkExt};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    net::SocketAddr,
    process::{Child, Command},
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
use tokio_tungstenite::{connect_async, tungstenite::protocol::Message as TungMessage};
use tracing::{error, info, warn};

// --- CONFIGURATION ---
const MIN_PORT: u16 = 8000;
const MAX_PORT: u16 = 9000; // Allows 1000 concurrent matches
const GAME_BINARY_PATH: &str = "./game_server"; 

// --- STATE MANAGEMENT ---
struct ServerProcess {
    child: Child,
    #[allow(dead_code)] // Suppress warning if not used in logs yet
    started_at: Instant,
    match_id: String,
    last_active: Arc<Mutex<Instant>>, 
    p1_token: String, 
    p2_token: String,
}

struct AppState {
    active_servers: Mutex<HashMap<u16, ServerProcess>>,
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

// NEW: Request DTO for the fail-safe
#[derive(Deserialize)]
struct DeallocateRequest {
    match_id: String,
    token: String,
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter("allocator=debug,tower_http=debug")
        .init();

    // 1. Initialize State
    let state = Arc::new(AppState {
        active_servers: Mutex::new(HashMap::new()),
        // Render provides this, fallback to localhost for dev
        public_host: std::env::var("RENDER_EXTERNAL_HOSTNAME")
            .unwrap_or_else(|_| "localhost:10000".to_string()), 
    });

    // 2. Start Background Reaper (Cleans up crashed/idle games)
    let reaper_state = state.clone();
    tokio::spawn(async move {
        run_reaper(reaper_state).await;
    });

    // 3. Setup Routes
    let app = Router::new()
        .route("/allocate", post(allocate_server))    // Start Game
        .route("/deallocate", post(deallocate_server)) // NEW: Stop Game (Fail-safe)
        .route("/play/:match_id", any(proxy_handler)) // WebSocket Proxy
        .with_state(state);

    // 4. Start Server
    // Listen on 0.0.0.0:10000 (Matches your Loco Controller config)
    let addr = SocketAddr::from(([0, 0, 0, 0], 10000));
    info!("Allocator Service listening on {}", addr);
    
    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}

// --- HANDLERS ---

// 1. ALLOCATE: Spawns a new game process
async fn allocate_server(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<AllocateRequest>,
) -> impl IntoResponse {
    let mut servers = state.active_servers.lock().unwrap();

    // Find a free port
    let port = (MIN_PORT..MAX_PORT).find(|p| !servers.contains_key(p));

    let port = match port {
        Some(p) => p,
        None => {
            error!("Allocation failed: No ports available!");
            return (StatusCode::SERVICE_UNAVAILABLE, "No ports available").into_response();
        }
    };

    info!("Spawning match {} on port {}", payload.match_id, port);

    // Spawn process
    let spawn_result = Command::new(GAME_BINARY_PATH)
        .args(&[
            "--port", &port.to_string(),
            "--p1-token", &payload.p1_token,
            "--p2-token", &payload.p2_token,
            "--match-id", &payload.match_id,
        ])
        .stdout(std::process::Stdio::inherit())
        .stderr(std::process::Stdio::inherit())
        .spawn();

    match spawn_result {
        Ok(child) => {
            // Register process
            servers.insert(port, ServerProcess {
                child,
                started_at: Instant::now(),
                match_id: payload.match_id.clone(),
                last_active: Arc::new(Mutex::new(Instant::now())), 
                p1_token: payload.p1_token.clone(),
                p2_token: payload.p2_token.clone(),
            });

            // Return connection URL (wss if using SSL, ws otherwise)
            // Note: Render usually handles SSL termination, so 'ws://' internally is fine, 
            // but the client sees 'wss://'. 
            // If your public_host includes the protocol, use it, otherwise assume wss:// for production.
            let protocol = if state.public_host.contains("localhost") { "ws" } else { "wss" };
            let connect_url = format!("{}://{}/play/{}", protocol, state.public_host, payload.match_id);

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

// 2. DEALLOCATE: Kills a specific game process (The Fail-Safe)
async fn deallocate_server(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<DeallocateRequest>,
) -> impl IntoResponse {
    let mut servers = state.active_servers.lock().unwrap();
    let mut target_port = None;

    // Search for the port associated with this match_id
    for (port, process) in servers.iter() {
        if process.match_id == payload.match_id {
            if payload.token == process.p1_token || payload.token == process.p2_token {
                target_port = Some(*port);
            } else {
                warn!("Unauthorized deallocate attempt for match {}", payload.match_id);
                return (StatusCode::UNAUTHORIZED, "Invalid token").into_response();
            }
            break;
        }
    }

    if let Some(port) = target_port {
        if let Some(mut process) = servers.remove(&port) {
            info!("MANUAL KILL: Stopping match {} on port {}", payload.match_id, port);
            
            // Kill and wait to prevent zombies
            let _ = process.child.kill(); 
            let _ = process.child.wait(); 
            
            return (StatusCode::OK, "Game stopped").into_response();
        }
    }

    warn!("Deallocate requested for unknown match: {}", payload.match_id);
    (StatusCode::NOT_FOUND, "Match not found").into_response()
}

// 3. PROXY: WebSocket Handoff
async fn proxy_handler(
    ws: WebSocketUpgrade,
    Path(match_id): Path<String>,
    Query(params): Query<HashMap<String, String>>,
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    let (target_port, activity_tracker) = {
        let servers = state.active_servers.lock().unwrap();
        match servers.iter().find(|(_, p)| p.match_id == match_id) {
            Some((port, process)) => (*port, process.last_active.clone()), 
            None => return (StatusCode::NOT_FOUND, "Match not found").into_response(),
        }
    };

    let token = params.get("token").cloned().unwrap_or_default();

    ws.on_upgrade(move |socket| async move {
        handle_proxy(socket, target_port, token, activity_tracker).await;
    })
}

async fn handle_proxy(
    mut client_socket: WebSocket, 
    port: u16, 
    token: String, 
    last_active: Arc<Mutex<Instant>> 
) {
    let local_url = format!("ws://127.0.0.1:{}/?token={}", port, token);
    
    match connect_async(local_url).await {
        Ok((mut game_socket, _)) => {
            let (mut client_sender, mut client_receiver) = client_socket.split();
            let (mut game_sender, mut game_receiver) = game_socket.split();

            let touch_activity = || {
                if let Ok(mut t) = last_active.lock() { *t = Instant::now(); }
            };

            // Client -> Game
            let client_to_game = async {
                while let Some(Ok(msg)) = client_receiver.next().await {
                    touch_activity();
                    let tungsten_msg = match msg {
                        Message::Text(t) => TungMessage::Text(t),
                        Message::Binary(b) => TungMessage::Binary(b),
                        Message::Close(_) => {
                            let _ = game_sender.close().await;
                            break;
                        },
                        _ => continue,
                    };
                    if game_sender.send(tungsten_msg).await.is_err() { break; }
                }
            };

            // Game -> Client
            let game_to_client = async {
                while let Some(Ok(msg)) = game_receiver.next().await {
                    touch_activity();
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

            tokio::select! {
                _ = client_to_game => {},
                _ = game_to_client => {},
            }
        }
        Err(e) => error!("Proxy connection failed to port {}: {}", port, e),
    }
}

// --- BACKGROUND TASKS ---
async fn run_reaper(state: Arc<AppState>) {
    let check_interval = Duration::from_secs(2); // Check often
    let timeout_duration = Duration::from_secs(300);

    // CONFIG: Where is Loco?
    // In Docker/Render, this might be the internal service URL
    let callback_url = std::env::var("LOCO_CALLBACK_URL")
        .unwrap_or_else(|_| "http://localhost:3000/api/matchmaking/internal_finish".to_string());
    
    let internal_secret = std::env::var("INTERNAL_API_KEY")
        .unwrap_or_else(|_| "super-secret-key".to_string());

    let client = reqwest::Client::new();

    loop {
        tokio::time::sleep(check_interval).await;
        
        let mut servers = state.active_servers.lock().unwrap();
        let mut ports_to_free = Vec::new();

        for (port, process) in servers.iter_mut() {
            let mut should_kill = false;

            // 1. Check if process exited naturally (Game Over)
            match process.child.try_wait() {
                Ok(Some(_)) => {
                    info!("Process on port {} exited naturally (Game Over)", port);
                    
                    // --- THE FIX: Notify Loco ---
                    let m_id = process.match_id.clone();
                    let url = callback_url.clone();
                    let secret = internal_secret.clone();
                    let c = client.clone();

                    // Fire and forget: Tell Loco to update DB to 'finished'
                    tokio::spawn(async move {
                        let _ = c.post(&url)
                            .json(&serde_json::json!({ 
                                "match_id": m_id,
                                "secret": secret
                            }))
                            .send()
                            .await;
                    });
                    // ----------------------------

                    ports_to_free.push(*port);
                    continue; 
                },
                Ok(None) => {}, // Still running
                Err(_) => {},
            }

            // 2. Check for idle timeout (Zombie)
            if let Ok(last_active) = process.last_active.lock() {
                if last_active.elapsed() > timeout_duration {
                    warn!("TIMEOUT: Killing match {} (Idle)", process.match_id);
                    should_kill = true;
                }
            }

            if should_kill {
                if let Err(e) = process.child.kill() {
                    error!("Kill failed: {}", e);
                } else {
                    let _ = process.child.wait();
                    
                    // Optional: You might want to notify Loco here too if you want 
                    // timeouts to show as "Game Over" instead of just dying.
                    ports_to_free.push(*port);
                }
            }
        }

        for port in ports_to_free {
            servers.remove(&port);
        }
    }
}