use bevy::state::app::StatesPlugin;
use bevy::{prelude::*, scene::ScenePlugin};
use bevy::app::ScheduleRunnerPlugin;
use bevy_rapier3d::prelude::*;
use clap::Parser;
use std::time::Duration;
use tokio::sync::{mpsc, broadcast};
use axum::{
    extract::{ws::{Message, WebSocket, WebSocketUpgrade}},
    response::IntoResponse,
    routing::any,
    Router,
};
use futures_util::{StreamExt, SinkExt};
use std::net::SocketAddr;
use std::sync::Arc;
use bevy::prelude::{Res,State};
#[path = "../lib.rs"] 
mod root_logic;
use root_logic::ClientMessage;

// --- 1. DEFINE RESOURCES ---

#[derive(Resource)]
pub struct BrowserInbound(pub mpsc::UnboundedReceiver<Vec<u8>>);

#[derive(Resource)]
pub struct BrowserOutbound(pub broadcast::Sender<Vec<u8>>);

#[derive(Resource)]
pub struct GameTokens {
    pub p1: String,
    pub p2: String,
    pub match_id: String,
}

// --- 2. CLI ARGUMENTS ---
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    #[arg(long, default_value_t = 8000)]
    port: u16,

    #[arg(long, default_value = "")]
    p1_token: String,

    #[arg(long, default_value = "")]
    p2_token: String,

    #[arg(long, default_value = "")]
    match_id: String,
}

// --- 3. MAIN ENTRY POINT ---
fn main() {
    let args = Args::parse();
    println!("Server starting on port {} | P1: {} | P2: {} | match_id: {}", args.port, args.p1_token, args.p2_token, args.match_id);

    // -- A. Setup Channels --
    
    // 1. INBOUND (Clients -> Bevy): Standard MPSC (Many inputs, one consumer)
    let (tx_to_bevy, rx_to_bevy) = mpsc::unbounded_channel();
    
    // 2. OUTBOUND (Bevy -> Clients): BROADCAST (One producer, many listeners)
    // Capacity 100 prevents laggy clients from crashing the server
    let (tx_from_bevy, _) = broadcast::channel::<Vec<u8>>(100);

    // -- B. Start WebSocket Server --
    let port = args.port;
    let tx_to_bevy_clone = tx_to_bevy.clone();
    let tx_from_bevy_clone = tx_from_bevy.clone();

    std::thread::spawn(move || {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            start_network_listener(port, tx_to_bevy_clone, tx_from_bevy_clone).await;
        });
    });

    // -- C. Setup Bevy App --
    let mut app = App::new();

    app.add_plugins(MinimalPlugins.set(ScheduleRunnerPlugin::run_loop(Duration::from_secs_f64(1.0 / 60.0))));

    app.add_plugins((
        AssetPlugin::default(),
        HierarchyPlugin::default(),
        TransformPlugin::default(),
        ScenePlugin::default(),
        bevy::log::LogPlugin::default(),
        StatesPlugin
    ));

    app.init_asset::<Mesh>();
    app.init_asset::<Scene>();
    app.init_asset::<StandardMaterial>();

    app.add_plugins(RapierPhysicsPlugin::<NoUserData>::default()).insert_resource(RapierConfiguration {
           gravity: Vec3::new(0.0, -9.81, 0.0),
           timestep_mode: TimestepMode::Fixed { dt: 1.0 / 60.0, substeps: 1 },
           physics_pipeline_active: true,
           query_pipeline_active: true,
           scaled_shape_subdivision: 2, 
           force_update_from_transform_changes: false, 
       });

    // Insert Resources
    app.insert_resource(BrowserInbound(rx_to_bevy));
    app.insert_resource(BrowserOutbound(tx_from_bevy)); // Bevy gets the Sender
    app.insert_resource(GameTokens {
        p1: args.p1_token,
        p2: args.p2_token,
        match_id: args.match_id
    });

    // Add your game logic
    // app.add_plugins(server::NineBallServerPlugin); 
    
       app
       .insert_state(WhoseMove::Player1)
       .insert_state(GamePhase::PreShot);
    app.add_systems(Update, broadcast_state_to_clients)
    .add_systems(Update, handle_incoming_network_messages);

    app.insert_resource(GameState::default());
    app.add_systems(Update, update_gamestate);
    app.add_plugins(NineBallRuleset);
    app.run();
}

// Add this component/system to send updates
fn broadcast_state_to_clients(
    game_state: Res<GameState>,
    network_out: Res<BrowserOutbound>,
) {
    // 1. Serialize the current GameState
    // We use a "Match" wrapper or just the raw state, depending on your client expectation.
    // Based on your client code: bincode::deserialize::<GameState>(&data)
    match bincode::serialize(&*game_state) {
        Ok(data) => {
            // 2. Send to the Tokio listener via the channel
            // We ignore errors because if no clients are connected, send fails (which is fine)
            let _ = network_out.0.send(data);
        },
        Err(e) => {
            eprintln!("Failed to serialize GameState: {}", e);
        }
    }
}


use bevy_rapier3d::prelude::Velocity; // Ensure you have this import

fn update_gamestate(
    // FIX 1: Only ask for ResMut once. You can read AND write to this.
    mut gamestate: ResMut<GameState>, 
    // FIX 2: Query Velocity too, so the client knows how fast balls are moving
    pool_ball_query: Query<(&Transform, &Velocity, &PoolBalls)>, 
    cue_ball_query: Query<(&Transform, &Velocity), With<CueBall>>
) {
    let mut ball_vec = vec![];

    // 1. Collect Normal Balls
    for (t, v, p) in pool_ball_query.iter() {
        ball_vec.push(BallData {
            number: p.0,
            position: t.translation,
            // actually send velocity (v.linvel) instead of ZERO
            velocity: v.linvel, 
            rotation: t.rotation,
            is_cue: false,
        });
    }

    // 2. Collect Cue Ball
    if let Ok((t, v)) = cue_ball_query.get_single() {
        ball_vec.push(BallData {
            number: 0,
            position: t.translation,
            velocity: v.linvel,
            rotation: t.rotation,
            is_cue: true,
        });
    }

    gamestate.balls = ball_vec;
}
fn handle_incoming_network_messages(
    // The channel we created in main()
    mut inbound: ResMut<BrowserInbound>, 
    // The state we need to update
    // Optional: Events if you use them to trigger physics
    // mut shot_events: EventWriter<ShotEvent>, 
    mut commands: Commands,
    mut cue_ball_query: Query<Entity, With<CueBall>>
) {
    // Loop until the channel is empty for this frame
    while let Ok(bytes) = inbound.0.try_recv() {
        match bincode::deserialize::<ClientMessage>(&bytes) {
            Ok(message) => {
                println!("Server received: {:?}", message);

                match message {
                    ClientMessage::Shot { power, direction, angvel } => {
                        // Apply the shot logic directly to the state
                        // OR trigger a physics event
                        println!("Processing shot: Power {}", power);
                        if let Ok(cue_ball) = cue_ball_query.get_single_mut() {
                            commands.entity(cue_ball).insert(Velocity {linvel: direction * power, angvel: Vec3::ZERO});
                        }
                    },
                    ClientMessage::BallPlacement { position } => {
                        println!("Moving cue ball to: {}", position);

                           if let Ok(cue_ball) = cue_ball_query.get_single_mut() {
                            commands.entity(cue_ball).insert(Transform::from_translation(position));
                        }
                    }
                    _ => {}
                }
            },
            Err(e) => eprintln!("Failed to deserialize client message: {}", e),
        }
    }
}


// --- 4. NETWORK LOGIC ---
#[derive(Clone)]
struct NetworkState {
    // Channel to send data TO Bevy (Client Input)
    to_bevy: mpsc::UnboundedSender<Vec<u8>>,
    // Channel to subscribe to data FROM Bevy (Game State Updates)
    from_bevy_broadcast: broadcast::Sender<Vec<u8>>,
}

async fn start_network_listener(
    port: u16,
    tx_to_bevy: mpsc::UnboundedSender<Vec<u8>>,
    tx_from_bevy: broadcast::Sender<Vec<u8>>,
) {
    let state = NetworkState {
        to_bevy: tx_to_bevy,
        from_bevy_broadcast: tx_from_bevy,
    };

    let app = Router::new()
        .route("/", any(ws_handler))
        .with_state(state);

    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    println!("WebSocket Listener bound to {}", addr);

    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}

async fn ws_handler(
    ws: WebSocketUpgrade,
    axum::extract::State(state): axum::extract::State<NetworkState>,
) -> impl IntoResponse {
    ws.on_upgrade(|socket| handle_socket(socket, state))
}

async fn handle_socket(socket: WebSocket, state: NetworkState) {
    let (mut sender, mut receiver) = socket.split();

    // Subscribe to the broadcast channel specifically for THIS connection
    let mut my_rx = state.from_bevy_broadcast.subscribe();

    loop {
        tokio::select! {
            // 1. INCOMING: Client (WASM) -> Bevy
            // FIX: Match Binary, not Text!
            Some(msg) = receiver.next() => {
                match msg {
                    Ok(Message::Binary(data)) => {
                        // Forward raw bytes to Bevy
                        let _ = state.to_bevy.send(data);
                    }
                    Ok(Message::Close(_)) => break,
                    // Ignore Text/Ping/Pong
                    _ => {}
                }
            }

            // 2. OUTGOING: Bevy -> Client (WASM)
            // FIX: Send Binary, not Text!
            Ok(msg) = my_rx.recv() => {
                // 'msg' is already Vec<u8> (Bincode bytes)
                if sender.send(Message::Binary(msg)).await.is_err() {
                    break;
                }
            }
        }
    }
}
use nine_ball_game::{GameState, WhoseMove};
use nine_ball_game::{TABLE_WIDTH, TABLE_LENGTH, FRICTION_COEFF, TABLE_FRICTION_COEFF, BALL_FRICTION_COEFF, CUE_BALL_RADIUS, STANDARD_BALL_RADIUS};
// ... Player struct definition
fn setup_physics_for_nine_ball(mut commands: Commands  ) {

    
/* Create the ground. */
    commands
    .spawn(RigidBody::Fixed)
        .insert(Collider::cuboid(TABLE_WIDTH, 0.0, TABLE_LENGTH))
      //  .insert(Friction{coefficient: FRICTION_COEFF, combine_rule: CoefficientCombineRule::Average})
      .insert(Friction::coefficient(TABLE_FRICTION_COEFF))
        .insert(TransformBundle::from(Transform::from_xyz(0.0, 0.0, 0.0)));


      commands
    .spawn(RigidBody::Fixed)
        .insert(Collider::cuboid(TABLE_WIDTH, 0.0, TABLE_LENGTH))
      //  .insert(Friction{coefficient: FRICTION_COEFF, combine_rule: CoefficientCombineRule::Average})
      .insert(Friction::coefficient(TABLE_FRICTION_COEFF))
        .insert(TransformBundle::from(Transform::from_xyz(0.0, 3.0, 0.0)));

    //create the walls
    commands
    .spawn(RigidBody::Fixed)
    .insert(Collider::cuboid(WALL_DIMENSIONS.half_size.x, WALL_DIMENSIONS.half_size.y, WALL_DIMENSIONS.half_size.z))
    .insert(TransformBundle::from_transform(Transform::from_translation(Vec3::new(TABLE_WIDTH, 0.0, TABLE_WIDTH))))
    .insert(Restitution {coefficient: 1.0, combine_rule: CoefficientCombineRule::Max});

    commands
    .spawn(RigidBody::Fixed)
    .insert(Collider::cuboid(WALL_DIMENSIONS.half_size.x, WALL_DIMENSIONS.half_size.y, WALL_DIMENSIONS.half_size.z))
    .insert(TransformBundle::from_transform(Transform::from_translation(Vec3::new(TABLE_WIDTH, 0.0, -TABLE_WIDTH))))
.insert(Friction::coefficient(FRICTION_COEFF))
.insert(Restitution {coefficient: 1.0, combine_rule: CoefficientCombineRule::Max});
commands
    .spawn(RigidBody::Fixed)
    .insert(Collider::cuboid(WALL_DIMENSIONS.half_size.x, WALL_DIMENSIONS.half_size.y, WALL_DIMENSIONS.half_size.z))
    .insert(TransformBundle::from_transform(Transform::from_translation(Vec3::new(-TABLE_WIDTH, 0.0, TABLE_WIDTH))))
    .insert(Restitution {coefficient: 1.0, combine_rule: CoefficientCombineRule::Max});

    commands
    .spawn(RigidBody::Fixed)
    .insert(Collider::cuboid(WALL_DIMENSIONS.half_size.x, WALL_DIMENSIONS.half_size.y, WALL_DIMENSIONS.half_size.z))
    .insert(TransformBundle::from_transform(Transform::from_translation(Vec3::new(-TABLE_WIDTH, 0.0, -TABLE_WIDTH))))
    .insert(Restitution {coefficient: 1.0, combine_rule: CoefficientCombineRule::Max});
    commands
    .spawn(RigidBody::Fixed)
    .insert(Collider::cuboid(WALL_DIMENSIONS.half_size.z, WALL_DIMENSIONS.half_size.y, WALL_DIMENSIONS.half_size.x))
    .insert(TransformBundle::from_transform(Transform::from_translation(Vec3::new(0.0, 0.0, -TABLE_LENGTH))))
    .insert(Restitution {coefficient: 1.0, combine_rule: CoefficientCombineRule::Max});
    commands
    .spawn(RigidBody::Fixed)
    .insert(Collider::cuboid(WALL_DIMENSIONS.half_size.z, WALL_DIMENSIONS.half_size.y, WALL_DIMENSIONS.half_size.x))
    .insert(TransformBundle::from_transform(Transform::from_translation(Vec3::new(0.0, 0.0, TABLE_LENGTH))))
    .insert(Restitution {coefficient: 1.0, combine_rule: CoefficientCombineRule::Max});

    //make aimer


    
    /* Create the cue ball. */
    commands
        .spawn(RigidBody::Dynamic)
        .insert(Collider::ball(CUE_BALL_RADIUS))
        .insert(BALL_RESTITUTION)
        //.insert(ColliderMassProperties::Mass(0.40))
        .insert(TransformBundle::from(Transform::from_xyz(0.0, CUE_BALL_RADIUS, -TABLE_WIDTH )))
        .insert(ColliderMassProperties::Mass(BALL_MASS))
        .insert(BALL_DAMPING)
        .insert(Friction::coefficient(BALL_FRICTION_COEFF))
        .insert(DEFAULT_VELOCITY)
        .insert(CueBall).insert(Ccd::enabled()).insert(ActiveEvents::COLLISION_EVENTS);




    // Create the pool balls
    commands
    .spawn(RigidBody::Dynamic)
    .insert(Collider::ball(STANDARD_BALL_RADIUS))
    .insert(BALL_RESTITUTION)
    .insert(PoolBalls(9))
    .insert(ColliderMassProperties::Mass(BALL_MASS))
    .insert(BALL_DAMPING)
    .insert(DEFAULT_VELOCITY)
    .insert(Friction::coefficient(BALL_FRICTION_COEFF))
    .insert(TransformBundle::from(Transform::from_xyz(0.0, STANDARD_BALL_RADIUS, TABLE_WIDTH))).insert(Ccd::enabled()).insert(ActiveEvents::COLLISION_EVENTS);

commands
    .spawn(RigidBody::Dynamic)
    .insert(Collider::ball(STANDARD_BALL_RADIUS))
    .insert(BALL_RESTITUTION)
    .insert(PoolBalls(4))
    .insert(ColliderMassProperties::Mass(BALL_MASS))
    .insert(BALL_DAMPING)
    .insert(DEFAULT_VELOCITY)
    .insert(Friction::coefficient(BALL_FRICTION_COEFF))
    .insert(TransformBundle::from(Transform::from_xyz(STANDARD_BALL_RADIUS * 2.0, STANDARD_BALL_RADIUS, TABLE_WIDTH))).insert(Ccd::enabled()).insert(ActiveEvents::COLLISION_EVENTS);

    commands
    .spawn(RigidBody::Dynamic)
    .insert(Collider::ball(STANDARD_BALL_RADIUS))
    .insert(BALL_RESTITUTION)
    .insert(PoolBalls(5))
    .insert(ColliderMassProperties::Mass(BALL_MASS))
    .insert(BALL_DAMPING)
    .insert(DEFAULT_VELOCITY)
    .insert(Friction::coefficient(BALL_FRICTION_COEFF))
    .insert(TransformBundle::from(Transform::from_xyz(-STANDARD_BALL_RADIUS * 2.0, STANDARD_BALL_RADIUS, TABLE_WIDTH))).insert(Ccd::enabled()).insert(ActiveEvents::COLLISION_EVENTS);

    commands
    .spawn(RigidBody::Dynamic)
    .insert(Collider::ball(STANDARD_BALL_RADIUS))
    .insert(BALL_RESTITUTION)
    .insert(PoolBalls(2))
    .insert(ColliderMassProperties::Mass(BALL_MASS))
    .insert(BALL_DAMPING)
    .insert(DEFAULT_VELOCITY)
    .insert(Friction::coefficient(BALL_FRICTION_COEFF))
    .insert(TransformBundle::from(Transform::from_xyz(STANDARD_BALL_RADIUS, STANDARD_BALL_RADIUS, TABLE_WIDTH  - STANDARD_BALL_RADIUS * 2.0))).insert(Ccd::enabled()).insert(ActiveEvents::COLLISION_EVENTS);

    commands
    .spawn(RigidBody::Dynamic)
    .insert(Collider::ball(STANDARD_BALL_RADIUS))
    .insert(BALL_RESTITUTION)
    .insert(PoolBalls(3))
    .insert(ColliderMassProperties::Mass(BALL_MASS))
    .insert(BALL_DAMPING)
    .insert(DEFAULT_VELOCITY)
    .insert(Friction::coefficient(BALL_FRICTION_COEFF))
    .insert(TransformBundle::from(Transform::from_xyz(-STANDARD_BALL_RADIUS, STANDARD_BALL_RADIUS, TABLE_WIDTH - STANDARD_BALL_RADIUS * 2.0))).insert(Ccd::enabled()).insert(ActiveEvents::COLLISION_EVENTS);

    commands
    .spawn(RigidBody::Dynamic)
    .insert(Collider::ball(STANDARD_BALL_RADIUS))
    .insert(BALL_RESTITUTION)
    .insert(PoolBalls(6))
    .insert(ColliderMassProperties::Mass(BALL_MASS))
    .insert(BALL_DAMPING)
    .insert(DEFAULT_VELOCITY)
    .insert(Friction::coefficient(BALL_FRICTION_COEFF))
    .insert(TransformBundle::from(Transform::from_xyz(-STANDARD_BALL_RADIUS, STANDARD_BALL_RADIUS, TABLE_WIDTH + STANDARD_BALL_RADIUS * 2.0))).insert(Ccd::enabled()).insert(ActiveEvents::COLLISION_EVENTS);

    commands
    .spawn(RigidBody::Dynamic)
    .insert(Collider::ball(STANDARD_BALL_RADIUS))
    .insert(BALL_RESTITUTION)
    .insert(PoolBalls(7))
    .insert(ColliderMassProperties::Mass(BALL_MASS))
    .insert(BALL_DAMPING)
    .insert(DEFAULT_VELOCITY)
    .insert(Friction::coefficient(BALL_FRICTION_COEFF))
    .insert(TransformBundle::from(Transform::from_xyz(STANDARD_BALL_RADIUS , STANDARD_BALL_RADIUS, TABLE_WIDTH  + STANDARD_BALL_RADIUS * 2.0))).insert(Ccd::enabled()).insert(ActiveEvents::COLLISION_EVENTS);

commands
    .spawn(RigidBody::Dynamic)
    .insert(Collider::ball(STANDARD_BALL_RADIUS))
    .insert(BALL_RESTITUTION)
    .insert(PoolBalls(1))
    .insert(ColliderMassProperties::Mass(BALL_MASS))
    .insert(BALL_DAMPING)
    .insert(DEFAULT_VELOCITY)
    .insert(Friction::coefficient(BALL_FRICTION_COEFF))
    .insert(TransformBundle::from(Transform::from_xyz(0.0, STANDARD_BALL_RADIUS, TABLE_WIDTH - 4.0 * STANDARD_BALL_RADIUS))).insert(Ccd::enabled()).insert(ActiveEvents::COLLISION_EVENTS);

commands
.spawn(RigidBody::Dynamic)
.insert(Collider::ball(STANDARD_BALL_RADIUS))
.insert(BALL_RESTITUTION)
.insert(PoolBalls(8))
    .insert(ColliderMassProperties::Mass(BALL_MASS))
    .insert(BALL_DAMPING)
    .insert(DEFAULT_VELOCITY)
    .insert(Friction::coefficient(BALL_FRICTION_COEFF))
    .insert(TransformBundle::from(Transform::from_xyz(0.0, STANDARD_BALL_RADIUS, TABLE_WIDTH  + 4.0 * STANDARD_BALL_RADIUS))).insert(Ccd::enabled()).insert(ActiveEvents::COLLISION_EVENTS)
    ; 

}


// CHANGE: usize -> u32
#[derive(Component, PartialEq, PartialOrd, Eq, Ord, Debug, Clone, Copy, Hash)]
struct PoolBalls(u32); 

#[derive(Component)]
struct FloatingNumber(u32);

// CHANGE: usize -> u32
#[derive(Resource, Debug, Clone, PartialEq, Eq, Hash)]
struct PoolBallsOnTable(u32);

#[derive(Component)]
struct MainWindow;

#[derive(Component)]
struct SecondWindow;

#[derive(Component)]
struct CueBall;


#[derive(Component)]
struct Aimer;

#[derive(Component)]
struct SpinSelector;

#[derive(Component)]
struct MyGameCamera;

#[derive(Component)]
struct CurrentCuePosition;

#[derive(Component)]
struct ShotPower(f32,bool);

const BALL_MASS: f32 = 0.17;
const BALL_RESTITUTION: Restitution = Restitution::coefficient(1.00);
const DEFAULT_VELOCITY: Velocity =  Velocity {
    linvel: Vec3::ZERO,
    angvel: Vec3::ZERO

};
const BALL_DAMPING: Damping = Damping {
linear_damping: 0.2533301,
angular_damping: 0.253301
};
const WALL_DIMENSIONS: Cuboid = Cuboid{half_size: Vec3::new(0.02, 120.55, 0.6096)};
const WALL_MESH_DIMENSIONS: Cuboid = Cuboid{half_size: Vec3::new(0.02, 5.55, 0.6096)};
const BACK_WALL_MESH_DIMENSIONS: Cuboid = Cuboid{half_size: Vec3::new(0.6096, 120.55, 0.002)};
const TARGET_BALL_TORUS_DIMENSIONS: Torus = Torus{ minor_radius: 0.002 , major_radius: 0.06 };
const CAMERA_HEIGHT: Vec3 = Vec3 {x: 0.0, y: 1.97, z: 0.0};

#[derive(Component, PartialEq, PartialOrd, Eq, Ord, Debug, Clone, Copy, Hash)]
struct TargetBallTorus;



fn query_target_ball_torus_in_nine_ball(mut commands: Commands, lowest_numbered_ball: Res<State<CorrectObjectBall>>, torus_query: Query< (Entity, &Transform), With<TargetBallTorus>>, pool_ball_query: Query<(&PoolBalls, &Transform)>) {
    let lowest_num = lowest_numbered_ball.0.0;
    let toruses= torus_query.iter();
    for (ball, transform) in pool_ball_query.iter() {
        if ball.0 == lowest_num {
            for (torus, torus_transform) in toruses{
                commands.entity(torus).insert(TransformBundle::from(Transform {translation: transform.translation,rotation: torus_transform.rotation, ..default() }));
            }
            break
        }
    }
}

#[derive(States, Debug, Clone, PartialEq, Eq, Hash)]
struct CorrectObjectBall(PoolBalls);


pub struct GameMode;

impl Plugin for GameMode {
    fn build(&self, app: &mut App) {
        app
        
        .insert_state(GamePhase::PreShot)
        .insert_state(WhoseMove::Player1)
        .insert_state(Scratch(false))
    .insert_state(FirstContactHasBeenMade::NotYet)
        .add_systems(Update, despawn_pocketed_balls.run_if(in_state(GamePhase::InMotion)))
        .add_event::<HumanPlayerMoveStart>()
        .add_event::<ComputerPlayerMoveStart>()
        .add_event::<ShotMade>()
           .add_event::<ShotCompletedPhysics>();

    }
    
}

fn should_check_object_ball(first_contact: Res<State<FirstContactHasBeenMade>>, phase: Res<State<GamePhase>>) -> bool {
    return  first_contact.get() == &FirstContactHasBeenMade::NotYet && phase.get() == &GamePhase::InMotion
}

pub struct NineBallRuleset;

impl Plugin for NineBallRuleset {
    fn build(&self, app: &mut App) {
        app
        .add_systems(Startup, setup_physics_for_nine_ball)
        .add_plugins(GameMode)
        .insert_state(CorrectObjectBall(PoolBalls(1)))
        .insert_resource(PoolBallsOnTable(9))
        .insert_state(Winner(WhoseMove::Player1))
        .add_systems(PostUpdate, state_setter_in_nine_ball_game)
        .add_systems(Update, query_target_ball_torus_in_nine_ball)
          .add_systems(
            Update, // Needs to run after physics updates
            check_if_balls_still_rolling.run_if(in_state(GamePhase::InMotion)).after(PhysicsSet::StepSimulation))        
          .add_systems(
            OnEnter(GamePhase::PostShot),
            evaluate_shot_rules,
        )
        .add_systems(Last, (check_for_win_in_nine_ball, set_correct_object_ball_after_shot_in_nine_ball, tabulate_for_nine_ball).run_if(in_state(GamePhase::PostShot)).after(PhysicsSet::StepSimulation))//run if shot has been made and balls have stopped moving
        .add_systems(PostUpdate, check_for_correct_object_ball.run_if(in_state(FirstContactHasBeenMade::NotYet)).run_if(in_state(GamePhase::InMotion)).run_if(should_check_object_ball).after(PhysicsSet::StepSimulation))
       .add_systems(
  PostUpdate,
  clear_collision_events
    .run_if(in_state(GamePhase::PostShot))
    .after(PhysicsSet::StepSimulation)
);
        
    }
}

fn clear_collision_events(mut collision_events: ResMut<Events<CollisionEvent>>,) {

collision_events.clear();
}

       




const  EPS: f32 = 1e-2;

// run when state is InMotion

// run when first contact has not been made but ball is in motion
fn check_for_correct_object_ball(
    // We now read CollisionEvents
    mut collision_events: EventReader<CollisionEvent>,
    previous_contact: Res<State<FirstContactHasBeenMade>>,
    mut next_first_contact_state: ResMut<NextState<FirstContactHasBeenMade>>,
    mut next_scratch_state: ResMut<NextState<Scratch>>,
    // We need the current state of lowest_numbered_ball
    current_correct_object_ball: Res<State<CorrectObjectBall>>,
    // Queries to identify the types of entities involved in the collision
    cue_ball_query: Query<Entity, With<CueBall>>,
    pool_ball_query: Query<(Entity, &PoolBalls)>,
) {
    // Get the cue ball entity once
    let Some(cue_ball_entity) = cue_ball_query.iter().next() else {
        // Handle case where cue ball doesn't exist, though it usually should
        return;
    };

    let correct_object_ball_number = current_correct_object_ball.0;

    for event in collision_events.read() {
        if let CollisionEvent::Started(e1, e2, _flags) = event {
            let (collider1, collider2) = (e1, e2);

            // Check if one of the entities is the cue ball
            let (other_ball_entity, is_cue_ball_involved) = if collider1 == &cue_ball_entity {
                (collider2, true)
            } else if collider2 == &cue_ball_entity {
                (collider1, true)
            } else {
                continue // Neither is the cue ball, skip
            };

            if is_cue_ball_involved {
                // Check if the other entity is a pool ball
                if let Ok((_other_ball_entity_from_query, other_pool_ball_component)) = pool_ball_query.get(*other_ball_entity) {
                    
                    let collided_ball_number = other_pool_ball_component.0;
                    if previous_contact.get() == &FirstContactHasBeenMade::NotYet { // Check the next state to avoid double setting
                        next_first_contact_state.set(FirstContactHasBeenMade::Yes);
                        println!("FIRST CONTACT HAS BEEN MADE with ball {}", collided_ball_number);
                        
                        
                        // Now check if it's the CORRECT object ball
                    if collided_ball_number != correct_object_ball_number.0 {
                        println!("lowest numbered ball: {} first contact (incorrect) {:?}", correct_object_ball_number.0, collided_ball_number);
                        println!("scratch set by incorrect first contact");
                        next_scratch_state.set(Scratch(true)); // Set scratch state
                    } else {
                        println!("lowest numbered ball: {} first contact (correct) {}", correct_object_ball_number.0, collided_ball_number);
                        // Optional: If hitting the correct ball first means *no* scratch, ensure scratch isn't set.
                        // Or if you only set `scratch` if it's incorrect, this branch is fine.
                    }
                }

                    // BREAK HERE if you only care about the *very first* collision event of the frame
                    // However, typically you process all events in the reader.
                    // If your game logic only cares about the first *conceptual* contact in a turn,
                    // you might use a flag to prevent further processing after the first relevant one is found.
                    // For instance, if you have a `BallInPlay` state and you only care about the
                    // first contact *while* `BallInPlay` is active.
                }
            }
        }
    }
    collision_events.clear();
}

fn set_correct_object_ball_after_shot_in_nine_ball(ball_query: Query<&PoolBalls>, mut correct_ball_setter: ResMut<NextState<CorrectObjectBall>>) {
    let mut lowest = 10;

    for i in ball_query.iter() {
        if i.0 < lowest {
            lowest = i.0;
        }
    }

    correct_ball_setter.set(CorrectObjectBall(PoolBalls(lowest)));
    println!("{:?}", lowest);

}

/* fn ball_in_hand(mut commands: Commands, mut meshes: ResMut<Assets<Mesh>>, mut materials: ResMut<Assets<StandardMaterial>>, mut set_state: ResMut<NextState<Phase>>, cue_ball_query: Query<(Entity, &Transform), With<CueBall>>) {
    if let Ok((cue_ball_entity, cue_ball_transform))= cue_ball_query.get_single() {
     
         
    } else {

        commands
        .spawn(RigidBody::Dynamic)
        .insert(MaterialMeshBundle {mesh: meshes.add(Sphere::new(STANDARD_BALL_RADIUS)), material: materials.add(StandardMaterial::from_color(Color::WHITE)), ..default()})
        .insert(Collider::ball(CUE_BALL_RADIUS))
        .insert(BALL_RESTITUTION)
        //.insert(ColliderMassProperties::Mass(0.40))
        .insert(TransformBundle::from(Transform::from_xyz(0.0, STANDARD_BALL_RADIUS, 0.0)))
        .insert(ColliderMassProperties::Mass(BALL_MASS))
        .insert(BALL_DAMPING)
        .insert(CueBall);
    }
      /* Create the cue ball. */

    

} */

fn computer_ball_in_hand(mut commands: Commands, cue_ball_query: Query<Entity, With<CueBall>>,  mut set_state: ResMut<NextState<GamePhase>>) {
    
    if let Ok(cue_ball_entity) = cue_ball_query.get_single() {
        commands.entity(cue_ball_entity).despawn();
    }

    commands
        .spawn(RigidBody::Dynamic)
        .insert(Collider::ball(CUE_BALL_RADIUS))
        .insert(BALL_RESTITUTION)
        //.insert(ColliderMassProperties::Mass(0.40))
        .insert(TransformBundle::from(Transform::from_xyz(0.0, STANDARD_BALL_RADIUS, 0.0)))
        .insert(ColliderMassProperties::Mass(BALL_MASS))
        .insert(BALL_DAMPING)
        .insert(CueBall)
        .insert(Velocity { linvel: Vec3::ZERO, angvel: Vec3::ZERO })
        .insert(Transform::from_translation(Vec3::new(0.0, CUE_BALL_RADIUS, 0.0)));
    set_state.set(GamePhase::PreShot);
}



fn despawn_pocketed_balls(mut commands: Commands, cue_ball_query: Query<(Entity, &Transform), With<CueBall>>, pool_ball_query: Query<(Entity, &Transform),With<PoolBalls>> ) {
    if let Ok((cue_ball, cue_transform) )= cue_ball_query.get_single() {
        if cue_transform.translation.y < -10.0 {
            commands.entity(cue_ball).despawn();
        }
    }

    for (i, target_ball_transform) in  pool_ball_query.iter() {
        if target_ball_transform.translation.y < -10.0 {
            commands.entity(i).despawn();
        }
    }
} 

fn state_setter_in_nine_ball_game( mut shot_made_reader: EventReader<ShotMade>,mut shot_finished_reader: EventReader<ShotCompletedPhysics>, mut next_phase: ResMut<NextState<GamePhase>>, ) {
    
    
    
    for i in shot_made_reader.read() {
        next_phase.set(GamePhase::InMotion);
    }
    for i in shot_finished_reader.read() {
        println!("shot_complete");
    next_phase.set(GamePhase::PostShot);
    }
}




#[derive(Event)]
pub struct ShotCompletedPhysics; // Event to signal physics has settled



// --- System 2: Checking if Balls Have Stopped ---
// This system runs only when in the `InMotion` phase.
fn check_if_balls_still_rolling(
    current_phase: Res<State<GamePhase>>, // Read current Phase
    mut next_phase: ResMut<NextState<GamePhase>>,
    velocity_query: Query<&Velocity, With<PoolBalls>>,
    cue_ball_velocity_query:  Query<&Velocity, With<CueBall>>, // Query for all balls that should stop
    mut event_writer: EventWriter<ShotCompletedPhysics>, // Event to signal physics has settled
) {
    // Only proceed if the phase is InMotion
for cue_vel in cue_ball_velocity_query.iter() {
    if cue_vel.linvel.length_squared() > EPS * EPS || cue_vel.angvel.length_squared() > EPS * EPS {
        return
}
}
    // Check if any ball is still moving
    for vel in velocity_query.iter() {
        if vel.linvel.length_squared() > EPS * EPS || vel.angvel.length_squared() > EPS * EPS {
            // A ball is still moving, so we don't proceed.
            return;
        }
    }

    // If we reach this point, all balls have stopped.
    println!("All balls stopped. Transitioning to Phase::ShotEvaluation.");
    event_writer.send(ShotCompletedPhysics);
}

// --- System 3: Evaluating Shot Rules ---
// This system runs once when entering the `ShotEvaluation` phase.
fn evaluate_shot_rules(
    first_contact_made: Res<State<FirstContactHasBeenMade>>, // Read resource
    mut scratch: ResMut<NextState<Scratch>>, // Read/Write resource
    // Add other resources/queries needed for other rules (e.g., pocketed balls, correct object ball hit)
) {
    println!("Evaluating shot rules...");

    // Rule: Scratch if no valid first contact was made
    if first_contact_made.get() == &FirstContactHasBeenMade::NotYet {
        println!("SCRATCH: No first contact was made!");
        scratch.set( Scratch(true)); // Set resource directly
    } else {
        println!("First contact was made.");
        // Add logic for other scratch conditions or valid shot checks
    }

    // After evaluating all rules, transition to PostShot (or whatever is next)
    println!("Shot evaluation complete. Transitioning to Phase::PostShot.");
}



fn tabulate_for_nine_ball( mut first_contact: ResMut<NextState<FirstContactHasBeenMade>>, check_first_contact: Res<State<FirstContactHasBeenMade>> ,is_scratch: Res<State<Scratch>> ,mut scratch_setter: ResMut<NextState<Scratch>>, mut next_shooter: ResMut<NextState<WhoseMove>>, current_shooter: Res<State<WhoseMove>>, mut next_phase: ResMut<NextState<GamePhase>>,mut balls_on_table: ResMut<PoolBallsOnTable>, ball_query: Query<Entity, With<PoolBalls>>, cue_ball_query: Query<Entity, With<CueBall>>) {

    let mut change_shooter = true;
    let mut scratch = false;
   
    let ball_count = ball_query.iter().count() as u32;
    if ball_count < balls_on_table.0 {
        
        change_shooter = false;
    }

    balls_on_table.0 = ball_count;

    if let Ok(cue) =  cue_ball_query.get_single() {
        
    } else {
        scratch = true;
        change_shooter = true;
    }
    println!("{:?} {:?}", is_scratch.get().0, change_shooter);
    if is_scratch.get().0 || scratch {
        next_phase.set(GamePhase::BallInHand);
    } else {
        println!("next phase set");
        next_phase.set(GamePhase::PreShot);
    }

    if change_shooter || is_scratch.get().0 {
        match current_shooter.get() {
            WhoseMove::Player1 => next_shooter.set(WhoseMove::Player2),
            WhoseMove::Player2 => next_shooter.set(WhoseMove::Player1),
        };  
    };
    first_contact.set(FirstContactHasBeenMade::NotYet);
    scratch_setter.set(Scratch(false));
}


//run before tabulate
fn check_for_win_in_nine_ball(pool_ball_query: Query<&PoolBalls>, is_scratch: Res<State<Scratch>>, whose_turn: Res<State<WhoseMove>>, mut winner: ResMut<NextState<Winner>>) {
    for i in pool_ball_query.iter() {
        if i.0 == 9{
            return
        }
    }
    if is_scratch.get().0 {
        match whose_turn.get() {
            WhoseMove::Player1 => winner.set(Winner(WhoseMove::Player2)),
            WhoseMove::Player2 => winner.set(Winner(WhoseMove::Player1)),
        };
    } else {
        winner.set(Winner(whose_turn.get().clone()));
    };
}

//
fn setup_nine_ball_game() {

}

#[derive(Event)]
pub struct HumanPlayerMoveStart;



#[derive(Event)]
pub struct ComputerPlayerMoveStart;


#[derive(Event)]
pub struct ShotMade;



#[derive(States, Debug, Clone, PartialEq, Eq, Hash)]
enum MyAppState {
    LoadingScreen,
    MainMenu,
    InGame(GameVariant),
    PostGame
}

#[derive(States, Debug, Clone, PartialEq, Eq, Hash)]
enum GameVariant {
    nine_ball,
    eight_ball,
    straight_pool,
    one_pocket,
}


use nine_ball_game::{BallData,GamePhase};




#[derive(States, Debug, Clone, PartialEq, Eq, Hash)]
enum FirstContactHasBeenMade {
    Yes,
    NotYet
}


#[derive(States, Debug, Clone, PartialEq, Eq, Hash)]
struct Scratch(bool);

#[derive(States, Debug, Clone, PartialEq, Eq, Hash)]
struct Winner(WhoseMove);

