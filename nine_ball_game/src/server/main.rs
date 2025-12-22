// src/server/main.rs
mod ws_gateway;

use bevy::prelude::*;
use bevy_rapier3d::prelude::*;
use serde::{Deserialize, Serialize};
use serde_json::from_slice;

use ws_gateway::{start_ws_gateway, BrowserInbound, BrowserOutbound, ConnectionMap, SessionId};
use nine_ball_game::{
    *
};
#[derive(Component)]
struct Player {
    id: SessionId,
    name: String,
    player_number: WhoseMove
}

fn main() {
    let mut app = App::new();

    app.add_plugins(MinimalPlugins);
    
    // Add these back if your game logic relies on them (e.g. Assets, Hierarchy)
    app.add_plugins(bevy::asset::AssetPlugin::default());
    app.add_plugins(bevy::hierarchy::HierarchyPlugin::default());
    app.add_plugins(bevy::state::app::StatesPlugin);
       app.add_plugins(RapierPhysicsPlugin::<NoUserData>::default())
       .insert_resource(RapierConfiguration {
           gravity: Vec3::new(0.0, -9.81, 0.0),
           timestep_mode: TimestepMode::Fixed { dt: 1.0 / 60.0, substeps: 1 },
           physics_pipeline_active: true,
           query_pipeline_active: true,
           scaled_shape_subdivision: 2, 
           force_update_from_transform_changes: false, 
       });

    // FIX: Listen on port 8000 to match the client's expectation
    let (inbound, outbound, map) = start_ws_gateway("0.0.0.0:8000".to_string());
    
    app.insert_resource(inbound)
       .insert_resource(outbound)
       .insert_resource(map)
       .insert_state(WhoseMove::Player1)
       .insert_state(GamePhase::PreShot);

    app.insert_resource(GameState::default())
       .add_systems(Update, assign_players_to_connections)
       .add_systems(Update, handle_client_messages)
       .add_systems(Update, sync_state_to_clients)
       .add_systems(Update, despawn_pocketed_balls);

    println!("Server running on ws://0.0.0.0:8000"); // Updated log
    app.add_plugins(NineBallRuleset);
    app.run();
}
use bevy::prelude::*;
use bevy::log::info; // Required for info! logging
use std::collections::{HashMap, HashSet}; // Required for assignment logic

// ... other imports

use nine_ball_game::WhoseMove;

// ... Player struct definition

// This system tracks connected sessions and assigns the first two to Player1 and Player2.
fn assign_players_to_connections(
    mut commands: Commands,
    connection_map: Res<ConnectionMap>, // The map of active SessionIds
    player_query: Query<&Player>,    // Query for already existing Player entities
) {
    // 1. Determine which player slots (P1, P2) are currently filled and which connections are active.
    let mut player_slots: HashMap<WhoseMove, SessionId> = HashMap::new();
    let mut active_sessions: HashSet<SessionId> = HashSet::new();

    // A. Gather currently assigned players from Bevy ECS
    for player in player_query.iter() {
        player_slots.insert(player.player_number.clone(), player.id);
    }
    
    // B. Gather all active connections from the gateway map resource
    // We use try_lock() because this map is a Mutex protecting shared state outside of Bevy's ECS.
    if let Ok(guard) = connection_map.0.try_lock() {
        for (&session_id, _) in guard.iter() {
            active_sessions.insert(session_id);
        }
    }
    
    // 2. Identify sessions that are connected but not yet assigned a Player component.
    // Get all session IDs that already have a Player component.
    let assigned_session_ids: HashSet<_> = player_slots.values().copied().collect();

    // Filter active sessions to find those without a Player component.
    let unassigned_sessions: Vec<SessionId> = active_sessions
        .into_iter()
        .filter(|id| !assigned_session_ids.contains(id))
        .collect();

    // 3. Assign the first two unassigned sessions to Player1 and Player2 slots.
    let available_slots = [WhoseMove::Player1, WhoseMove::Player2];
    let mut unassigned_iter = unassigned_sessions.into_iter();
    
    for player_number in available_slots.iter() {
        // Only assign if the player slot (Player1 or Player2) is currently vacant
        if !player_slots.contains_key(player_number) {
            // Try to pull the next unassigned session
            if let Some(session_id) = unassigned_iter.next() {
                let next_player_number = player_number.clone();
                
                info!("New player connected: {:?} assigned to {:?}", session_id, next_player_number);
                
                // Spawn a new Player entity with the assigned number
                commands.spawn(Player { 
                    id: session_id, 
                    name: format!("Player_{}", if next_player_number == WhoseMove::Player1 { 1 } else { 2 }), 
                    player_number: next_player_number 
                });
            } else {
                // No more unassigned sessions left, stop checking slots.
                break;
            }
        }
    }
}

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

fn handle_client_messages(
    mut commands: Commands, 
    mut inbound: ResMut<BrowserInbound>,
    mut cue_ball_query: Query<(&mut Velocity, &mut Transform), With<CueBall>>,
    player_query: Query<&Player>,
    current_turn: Res<State<WhoseMove>>,
    current_phase: Res<State<GamePhase>>,
    mut next_phase: ResMut<NextState<GamePhase>>,
    mut shot_event_writer: EventWriter<ShotMade>,
) {
    let current_shooter_number = current_turn.get();
    
    while let Ok((session_id, data)) = inbound.0.try_recv() {
        
        // Check for the synthetic JOIN message from ws_gateway first
        if let Ok(json_msg) = from_slice::<serde_json::Value>(&data) {
             if let Some(token) = json_msg.get("join").and_then(|v| v.as_str()) {
                 info!("Player joined with token: {} (Session {})", token, session_id);
                 // TODO: Here you can look up the token in Redis to verify identity
                 // For now, allow the assign_players_to_connections system to pick them up
                 continue;
             }
        }

        // Standard Game Logic
        let sender_player = player_query.iter().find(|p| p.id == session_id);
        
        if sender_player.is_none() {
            // If it's not a Join message and they aren't assigned, ignore
            continue;
        }

        let sender_player_number = &sender_player.unwrap().player_number;

        if let Ok(msg) = bincode::deserialize::<ClientMessage>(&data) {
            match msg {
                ClientMessage::Shot { power, direction, angvel } => {
                    if current_phase.get() != &GamePhase::PreShot { continue; }
                    if sender_player_number != current_shooter_number { continue; }

                    if let Ok((mut vel, _)) = cue_ball_query.get_single_mut() {
                        let impulse = direction.normalize() * power * 1.25;
                        vel.linvel = impulse;
                        vel.angvel = angvel;
                        shot_event_writer.send(ShotMade); 
                    }
                }
                ClientMessage::BallPlacement { position } => {
                     if current_phase.get() != &GamePhase::BallInHand { continue; }
                     if sender_player_number != current_shooter_number { continue; }

                     if let Ok((mut vel, mut transform)) = cue_ball_query.get_single_mut() {
                             transform.translation = position;
                             vel.linvel = Vec3::ZERO;
                             vel.angvel = Vec3::ZERO;
                     } else {
                        // (Cue ball spawn logic)
                        commands.spawn(RigidBody::Dynamic)
                            .insert(Collider::ball(CUE_BALL_RADIUS))
                            .insert(BALL_RESTITUTION)
                            .insert(TransformBundle::from_transform(Transform::from_translation(position)))
                            .insert(ColliderMassProperties::Mass(BALL_MASS))
                            .insert(BALL_DAMPING)
                            .insert(CueBall)
                            .insert(Velocity { linvel: Vec3::ZERO, angvel: Vec3::ZERO });
                    }
                    next_phase.set(GamePhase::PreShot); 
                }
                ClientMessage::Join { .. } => { }
            }
        } 
    }
}

fn sync_state_to_clients(
    mut outbound: ResMut<BrowserOutbound>,
    connection_map: Res<ws_gateway::ConnectionMap>,
    ball_query: Query<(&Transform, &Velocity, &PoolBalls)>, cue_ball_query: Query<(&Transform, &Velocity, &CueBall)>, phase: Res<State<GamePhase>>,whose_move: Res<State<WhoseMove>>
) {
    
    let mut state = GameState::default();
    let mut moving = false;
    
    for (t, v, b) in ball_query.iter() {
        if v.linvel.length_squared() > 0.01 || v.angvel.length_squared() > 0.01 {
            moving = true;
        }
        state.balls.push(BallData {
            number: b.0,
            position: t.translation,
            velocity: v.linvel,
            rotation: t.rotation,
            is_cue: false,
        });
    }

    for (t,v,c) in cue_ball_query.get_single() {
        if v.linvel.length_squared() > 0.01 || v.angvel.length_squared() > 0.01 {
            moving = true;
        }
        state.balls.push(BallData {
            number: 0,
            position: t.translation,
            velocity: v.linvel,
            rotation: t.rotation,
            is_cue: true,
        });
    }
    state.whose_move = whose_move.get().clone();
    state.phase = phase.get().clone();
    state.should_show_shot_controls = !moving; 

    if let Ok(bytes) = bincode::serialize(&state) {
        let map_guard = connection_map.0.try_lock();
        if let Ok(guard) = map_guard {
            for (&session_id, _) in guard.iter() {
                let _ = outbound.0.send((session_id, bytes.clone()));
            }
        }
    }
}


#[derive(Component, PartialEq, PartialOrd, Eq, Ord, Debug, Clone, Copy, Hash)]
struct PoolBalls(usize);

#[derive(Component)]
struct MainWindow;

#[derive(Component)]
struct SecondWindow;

#[derive(Component)]
struct CueBall;

#[derive(Component)]
struct FloatingNumber(usize);

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
const TABLE_FRICTION_COEFF: f32 = 1.00;
const FRICTION_COEFF: f32 = 1.0;
const BALL_FRICTION_COEFF:f32 = 1.0;
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
   
    let ball_count = ball_query.iter().count();
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


use nine_ball_game::GamePhase;


#[derive(States, Debug, Clone, PartialEq, Eq, Hash)]
enum FirstContactHasBeenMade {
    Yes,
    NotYet
}

#[derive(Resource, Debug, Clone, PartialEq, Eq, Hash)]
struct PoolBallsOnTable(usize);

#[derive(States, Debug, Clone, PartialEq, Eq, Hash)]
struct CorrectObjectBall(PoolBalls);

#[derive(States, Debug, Clone, PartialEq, Eq, Hash)]
struct Scratch(bool);

#[derive(States, Debug, Clone, PartialEq, Eq, Hash)]
struct Winner(WhoseMove);

