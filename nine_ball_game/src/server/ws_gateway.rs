// src/server/ws_gateway.rs
use std::collections::HashMap;
use std::sync::Arc;
use futures::{SinkExt, StreamExt};
use tokio::sync::{mpsc as tokio_mpsc, Mutex as TokioMutex};
use tokio_tungstenite::tungstenite::Message as WsMessage;
use tokio_tungstenite::accept_async;
use tokio::net::TcpListener;
use tokio::task;
use bevy::prelude::*;

/// Gateway-assigned id for each browser connection
pub type SessionId = u64;

/// Inbound bytes from browsers into Bevy
pub type BrowserInboundSender = tokio_mpsc::UnboundedSender<(SessionId, Vec<u8>)>;
pub type BrowserInboundReceiver = tokio_mpsc::UnboundedReceiver<(SessionId, Vec<u8>)>;

/// Outbound bytes from Bevy to browsers
pub type BrowserOutboundSender = tokio_mpsc::UnboundedSender<(SessionId, Vec<u8>)>;
pub type BrowserOutboundReceiver = tokio_mpsc::UnboundedReceiver<(SessionId, Vec<u8>)>;

/// Per-socket channel type used by the gateway write task
pub type ToSocketSender = tokio_mpsc::UnboundedSender<WsMessage>;

/// Internal shared map type (not a Bevy resource directly)
type InnerConnectionMap = Arc<TokioMutex<HashMap<SessionId, ToSocketSender>>>;

/// Bevy resource wrapper for the connection map
#[derive(Resource, Clone)]
pub struct ConnectionMap(pub InnerConnectionMap);

/// Bevy resource wrapper for the inbound channel
#[derive(Resource)]
pub struct BrowserInbound(pub BrowserInboundReceiver);

/// Bevy resource wrapper for the outbound channel sender
#[derive(Resource)]
pub struct BrowserOutbound(pub BrowserOutboundSender);


// ... (Type aliases and Resource wrappers remain the same)

/// Starts the async WebSocket gateway in a new dedicated Tokio thread.
pub fn start_ws_gateway(addr: String) -> (BrowserInbound, BrowserOutbound, ConnectionMap) {
    let (in_tx, in_rx) = tokio_mpsc::unbounded_channel();
    let (out_tx, out_rx) = tokio_mpsc::unbounded_channel();
    let inner_map: InnerConnectionMap = Arc::new(TokioMutex::new(HashMap::new()));
    
    let connection_map_for_bevy = inner_map.clone(); // Clone for the return value

    // Spawn dedicated thread for Tokio runtime to handle async networking
    std::thread::spawn(move || {
        let rt = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .expect("Failed to build Tokio runtime");

        rt.block_on(async move {
            let listener = TcpListener::bind(&addr).await.expect("WS gateway: Failed to bind");
            println!("WS gateway listening on ws://{}", addr);

            let map_clone_writer = inner_map.clone(); // Clone for Task 1
            
            // Task 1: Handle messages from Bevy (out_rx) and send to specific browsers
            task::spawn(async move {
                let mut out_rx = out_rx;
                while let Some((session_id, data)) = out_rx.recv().await {
                    let map = map_clone_writer.lock().await;
                    if let Some(tx) = map.get(&session_id) {
                        if let Err(e) = tx.send(WsMessage::Binary(data)) {
                            eprintln!("WS gateway: failed to send to socket channel: {}", e);
                        }
                    }
                }
            });

            let mut session_counter: SessionId = 0;
            // Task 2: Accept incoming connections (uses the original `inner_map`)
            loop {
                match listener.accept().await {
                    Ok((stream, addr)) => {
                        match accept_async(stream).await {
                            Ok(ws_stream) => {
                                session_counter += 1;
                                let session_id = session_counter;
                                let (mut ws_sink, mut ws_source) = ws_stream.split();
                                
                                let (to_socket_tx, mut to_socket_rx) = tokio_mpsc::unbounded_channel();
                                
                                // FIX: Clone the inner_map for this loop iteration before use/move
                                let current_map_ref = inner_map.clone(); 
                                
                                {
                                    let mut map = current_map_ref.lock().await;
                                    map.insert(session_id, to_socket_tx);
                                }

                                let in_tx_clone = in_tx.clone();
                                let connection_map_clone_read = current_map_ref.clone(); // Clone for read cleanup

                                // Read task: reads from the socket and sends to Bevy
                                let read_task = task::spawn(async move {
                                    while let Some(Ok(ws_msg)) = ws_source.next().await {
                                        match ws_msg {
                                            WsMessage::Binary(data) => {
                                                let _ = in_tx_clone.send((session_id, data));
                                            }
                                            WsMessage::Text(text) => {
                                                let _ = in_tx_clone.send((session_id, text.into_bytes()));
                                            }
                                            WsMessage::Close(_) => break,
                                            _ => {}
                                        }
                                    }
                                    // cleanup
                                    let mut map = connection_map_clone_read.lock().await;
                                    map.remove(&session_id);
                                    println!("WS gateway: session {} disconnected (read loop)", session_id);
                                });

                                let connection_map_clone_write = current_map_ref.clone(); // Clone for write cleanup

                                // Write task: receives from the to_socket channel and writes to the socket
                                let write_task = task::spawn(async move {
                                    while let Some(ws_msg) = to_socket_rx.recv().await {
                                        if let Err(e) = ws_sink.send(ws_msg).await {
                                            eprintln!("WS gateway: ws send error: {}", e);
                                            break;
                                        }
                                    }
                                    // cleanup
                                    let mut map = connection_map_clone_write.lock().await;
                                    map.remove(&session_id);
                                    println!("WS gateway: session {} disconnected (write loop)", session_id);
                                });

                                let _ = (read_task, write_task);
                                println!("WS gateway: accepted browser session {} from {}", session_id, addr);
                            }
                            Err(e) => {
                                eprintln!("WS gateway: handshake failed for {}: {}", addr, e);
                            }
                        }
                    }
                    Err(e) => {
                        eprintln!("WS gateway: accept error: {}", e);
                    }
                }
            }
        });
    });

    (BrowserInbound(in_rx), BrowserOutbound(out_tx), ConnectionMap(connection_map_for_bevy))
}