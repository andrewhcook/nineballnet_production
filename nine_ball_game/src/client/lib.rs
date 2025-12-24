use bevy::render::render_asset::RenderAssetUsages;
use bevy::sprite::{MaterialMesh2dBundle, Mesh2dHandle};
use bevy::window::PrimaryWindow;
// src/client/main.rs
use bevy::{color::palettes::css::WHITE, prelude::*};
use bevy::color::palettes::css; 
use bevy_rapier3d::prelude::*;
use ewebsock::{WsSender, WsReceiver, WsEvent, WsMessage};


#[path = "../lib.rs"] 
mod root_logic;

use root_logic::{
    
    
     BACK_WALL_MESH_DIMENSIONS, CUE_BALL_RADIUS, ClientMessage, GamePhase, GameState, STANDARD_BALL_RADIUS, TABLE_LENGTH, TABLE_WIDTH, TARGET_BALL_TORUS_DIMENSIONS, WALL_MESH_DIMENSIONS
};
use meshtext::{MeshGenerator, MeshText, TextSection as _};
use serde::{Deserialize, Serialize};

// --- Thread-Safe Network Client (for WASM target) ---
#[derive(Resource)]
struct NetworkClient {
    sender: WsSender,
    receiver: WsReceiver,
}

// UNSAFE: WsReceiver uses std::sync::mpsc::Receiver, which is not Send+Sync in Bevy's context.
// However, since the client targets WASM, which is single-threaded, this is acceptable
// for basic event consumption on the main thread.
unsafe impl Send for NetworkClient {}
unsafe impl Sync for NetworkClient {}

const CAMERA_HEIGHT: Vec3 = Vec3 {x: 0.0, y: 1.97, z: 0.0};
// --- Components (Visual Only) ---
#[derive(Component)]
struct VisualBall {
    number: u32,
}
#[derive(Component)]
struct MyGameCamera;
#[derive(Component)]
struct CueBallVisual;

// --- Shared Protocols (MUST Match Server) ---



#[derive(Serialize, Deserialize, Debug)]
pub struct ShotInfo {
    pub shot_power: f32,
    pub linvel: Vec3,
    pub angvel: Vec3,
}

#[derive(Serialize, Deserialize, Debug)]
pub enum TypeOfMessage {
    shot_info(ShotInfo),
    ball_in_hand_placement(Vec3),
}


#[derive(Component)]
struct CueBall;
#[derive(Component, Debug, Copy, Clone, Eq, PartialEq, Hash)]
struct PoolBalls(usize);
#[derive(Component)]
struct ShotPower(f32, bool);
#[derive(Component)]
struct Aimer;
#[derive(Component)]
struct ContactAngleVisual;
#[derive(Component)]
struct BallReactionVector;
#[derive(Component)]
struct FloatingNumber(usize);
#[derive(Component)]
struct TargetBallTorus;

#[derive(Component)]
struct SpinSelector;
#[derive(Component)]
struct SecondWindow;



fn setup_physics(mut commands: Commands, mut meshes: ResMut<Assets<Mesh>>,   mut materials: ResMut<Assets<StandardMaterial>>, mut fonts: ResMut<Assets<Font>>, ) {
    commands
    .spawn(RigidBody::Fixed)
      //  .insert(Friction{coefficient: FRICTION_COEFF, combine_rule: CoefficientCombineRule::Average})
      .insert(MaterialMeshBundle {mesh: meshes.add(Cuboid::from_corners(Vec3::new(2.25, 0.0, 4.5), Vec3::new(-2.25, 0.0, -4.5))), material: materials.add(StandardMaterial::from_color(Color::Hsla(Hsla::new(120.0 , 0.68, 0.93, 1.0)))), ..default()})
        .insert(TransformBundle::from(Transform::from_xyz(0.0, 0.0, 0.0)));


      commands
    .spawn(RigidBody::Fixed)
      //  .insert(Friction{coefficient: FRICTION_COEFF, combine_rule: CoefficientCombineRule::Average})
      .insert(MaterialMeshBundle {mesh: meshes.add(Cuboid::from_corners(Vec3::new(12.25, 0.0, 14.5), Vec3::new(-12.25, 0.0, -14.5))), material: materials.add(StandardMaterial::from_color(Color::Hsla(Hsla::new(120.0 , 0.68, 0.93, 1.0)))), ..default()})
        .insert(TransformBundle::from(Transform::from_xyz(0.0, 3.0, 0.0)));

    //create the walls
    commands
    .spawn(RigidBody::Fixed)
    .insert(MaterialMeshBundle {mesh: meshes.add(WALL_MESH_DIMENSIONS), material: materials.add(StandardMaterial::from_color(Color::Hsla(Hsla::new(30.0 ,0.60, 0.20, 1.0)))), ..default()})
    .insert(TransformBundle::from_transform(Transform::from_translation(Vec3::new(TABLE_WIDTH, 0.0, TABLE_WIDTH))));

    commands
    .spawn(RigidBody::Fixed)
    .insert(MaterialMeshBundle {mesh: meshes.add(WALL_MESH_DIMENSIONS), material: materials.add(StandardMaterial::from_color(Color::Hsla(Hsla::new(30.0 ,0.60, 0.20, 1.0)))), ..default()})
    .insert(TransformBundle::from_transform(Transform::from_translation(Vec3::new(TABLE_WIDTH, 0.0, -TABLE_WIDTH))));
commands
    .spawn(RigidBody::Fixed)
    .insert(MaterialMeshBundle {mesh: meshes.add(WALL_MESH_DIMENSIONS), material: materials.add(StandardMaterial::from_color(Color::Hsla(Hsla::new(30.0 ,0.60, 0.20, 1.0)))), ..default()})
    .insert(TransformBundle::from_transform(Transform::from_translation(Vec3::new(-TABLE_WIDTH, 0.0, TABLE_WIDTH))));

    commands
    .spawn(RigidBody::Fixed)
    .insert(MaterialMeshBundle {mesh: meshes.add(WALL_MESH_DIMENSIONS), material: materials.add(StandardMaterial::from_color(Color::Hsla(Hsla::new(30.0 ,0.60, 0.20, 1.0)))), ..default()})
    .insert(TransformBundle::from_transform(Transform::from_translation(Vec3::new(-TABLE_WIDTH, 0.0, -TABLE_WIDTH))));
    commands
    .spawn(RigidBody::Fixed)
    .insert(MaterialMeshBundle {mesh: meshes.add(BACK_WALL_MESH_DIMENSIONS), material: materials.add(StandardMaterial::from_color(Color::Hsla(Hsla::new(30.0 ,0.60, 0.20, 1.0)))), ..default()})
    .insert(TransformBundle::from_transform(Transform::from_translation(Vec3::new(0.0, 0.0, -TABLE_LENGTH))));
    commands
    .spawn(RigidBody::Fixed)
    .insert(MaterialMeshBundle {mesh: meshes.add(BACK_WALL_MESH_DIMENSIONS), material: materials.add(StandardMaterial::from_color(Color::Hsla(Hsla::new(30.0 ,0.60, 0.20, 1.0)))), ..default()})
    .insert(TransformBundle::from_transform(Transform::from_translation(Vec3::new(0.0, 0.0, TABLE_LENGTH))));

    //make aimer
    commands.spawn(ShotPower(1.0, true));
    commands.spawn(Aimer).insert(Sensor);
    
 //   commands.spawn(TargetBallTorus)
   // .insert(MaterialMeshBundle{mesh: meshes.add(TARGET_BALL_TORUS_DIMENSIONS), material: materials.add(StandardMaterial::from_color(Color::Hsla(Hsla::new(30.0 ,0.60, 0.20, 1.0)))), ..default()});

   // commands.spawn(TargetBallTorus).insert(MaterialMeshBundle{mesh: meshes.add(TARGET_BALL_TORUS_DIMENSIONS), material: materials.add(StandardMaterial::from_color(Color::Hsla(Hsla::new(30.0 ,0.60, 0.20, 1.0)))), ..default()});;

}



#[derive(Resource)]
struct ConnectionTicket {
    gateway_url: String,
    handoff_token: String,
}


use wasm_bindgen::prelude::*;


#[wasm_bindgen]
pub fn run_game(canvas_id: String, gateway_url: String, handoff_token: String) {
    let mut app = App::new();
    
    app.add_plugins(DefaultPlugins.set(WindowPlugin {
        primary_window: Some(Window {
            canvas: Some(format!("#{}", canvas_id)),
            fit_canvas_to_parent: true,
            ..default()
        }),
        ..default()
    }));

    // Pass the Loco ticket into Bevy as a Resource
    app.insert_resource(ConnectionTicket {
        gateway_url,
        handoff_token,
    });

    configure_app(&mut app);
    
    // In WASM, we use the specialized system that reads the Resource
    app.add_systems(Startup, connect_to_server_system);

    app.run();
}


fn connect_to_server_system(
    mut commands: Commands, 
    ticket: Res<ConnectionTicket>
) {
    connect_to_server(&mut commands, &ticket.gateway_url, &ticket.handoff_token);
}

fn configure_app(app: &mut App) {
    app.add_plugins(RapierPhysicsPlugin::<NoUserData>::default())
       .add_plugins(RapierDebugRenderPlugin::default())
       .insert_resource(GameState::default())
       .add_systems(Startup, (setup, spawn_pool_balls, setup_physics))
       .add_systems(Startup, setup_numbers_above_pool_balls.after(setup))
       .add_systems(Update, (
           handle_network, 
           render_gamestate,
           show_numbers_above_pool_balls, 
           rotate_numbers_around_pool_balls
       ))
       .add_systems(Update, (
           aim_system, 
 //          rotate_torus, 
           display_shot_power, 
           increase_shot_power
       ).run_if(should_show_player_shot_controls))
       .add_systems(Update, ball_in_hand.run_if(should_show_player_shot_controls))
       .add_systems(Update, despawn_aimer_polyline.run_if(should_not_show_player_shot_controls));
}



fn setup(mut commands: Commands, asset_server: Res<AssetServer>, mut meshes: ResMut<Assets<Mesh>>,   mut materials: ResMut<Assets<ColorMaterial>>) {
    // Add a camera
  commands.spawn((Camera3dBundle {
        transform: Transform::from_translation(CAMERA_HEIGHT).looking_at(Vec3::ZERO, Vec3::Y),
        ..Default::default()
    }, MyGameCamera));


    commands.spawn(PointLightBundle::default() );
    commands.spawn(PointLightBundle{transform: Transform::from_translation(Vec3::new(-10.0, 10.0, 0.0)), ..default()});
    commands.spawn(PointLightBundle{transform: Transform::from_translation(Vec3::new(10.0, 10.0, 0.0)), ..default()});
    commands.spawn(PointLightBundle{transform: Transform::from_translation(Vec3::new(0.0, 10.0, 10.0)), ..default()});
    commands.spawn(PointLightBundle{transform: Transform::from_translation(Vec3::new(0.0, 10.0, -10.0)), ..default()});

    
      commands.spawn(ContactAngleVisual).insert(Collider::polyline(vec![Vec3::ZERO, Vec3::ZERO], Some(vec![[0,1]]))).insert(Sensor);
    commands.spawn(BallReactionVector).insert(Collider::polyline(vec![Vec3::ZERO, Vec3::ZERO], Some(vec![[0,1]]))).insert(Sensor);
    
    
}


// --- Network and State Handling ---

fn connect_to_server(commands: &mut Commands, url: &str, token: &str) {
    // Append the token as a query parameter for the game server to validate
    let full_url = format!("{}?token={}", url, token);
    println!("Connecting to Game Server: {}", full_url);
    
    // Options::default() is fine for now
    match ewebsock::connect(full_url, ewebsock::Options::default()) {
        Ok((sender, receiver)) => {
            // REMOVED: sender.send(...) 
            // We cannot send yet! The socket is not open.
            
            commands.insert_resource(NetworkClient { sender, receiver });
            println!("Network Client Initialized - Waiting for Open event...");
        }
        Err(e) => eprintln!("Failed to connect: {}", e),
    }
}

fn handle_network(
    mut client: Option<ResMut<NetworkClient>>, 
    mut game_state: ResMut<GameState>,
) {
    if let Some(client) = client.as_mut() {
        // Loop through all available events
        while let Some(event) = client.receiver.try_recv() {
            match event {
                // 1. Connection is officially Open
                WsEvent::Opened => {
                    println!("WebSocket Connection Established!");
                    // IF your server requires a "join" message, send it HERE:
                    // client.sender.send(WsMessage::Text(r#"{"join": "WebPlayer"}"#.to_string()));
                }

                // 2. Binary Data (GameState updates)
                WsEvent::Message(WsMessage::Binary(data)) => {
                    if let Ok(new_state) = bincode::deserialize::<GameState>(&data) {
                        *game_state = new_state;
                    } else {
                        eprintln!("Failed to deserialize GameState (size: {} bytes)", data.len());
                    }
                }

                // 3. Text Data (Debugging/Chat)
                WsEvent::Message(WsMessage::Text(text)) => {
                    println!("Server says: {}", text);
                }

                // 4. Errors & Closing
                WsEvent::Error(e) => {
                    eprintln!("WebSocket Error: {}", e);
                }
                WsEvent::Closed => {
                    println!("WebSocket Disconnected.");
                }
                _ => {} // Handle Ping/Pong or Unknown
            }
        }
    }
}


fn should_show_player_shot_controls(current_gamestate: Res<GameState>) -> bool {
    current_gamestate.should_show_shot_controls
}

fn should_not_show_player_shot_controls(gamestate: Res<GameState>) -> bool{
    !gamestate.should_show_shot_controls
}

fn should_show_ball_in_hand(gamestate: Res<GameState>) -> bool {
    gamestate.phase == GamePhase::BallInHand 
}

// --- Network and State Handling ---

fn render_gamestate(mut commands: Commands, gamestate: Res<GameState>, cue_ball_query: Query<Entity, With<CueBall>>, pool_ball_query: Query<(Entity, &PoolBalls)>) {
     
     
     for i in &gamestate.balls{
        if i.is_cue {
            let cue_ball = cue_ball_query.single();
            commands.entity(cue_ball).insert(TransformBundle::from_transform(Transform::from_translation(i.position)));
        } else {
        if let Some( pool_ball )= pool_ball_query.iter().find(|(entity, pool_ball)| pool_ball.0 as u32 == i.number) {
            commands.entity(pool_ball.0).insert(TransformBundle::from_transform(Transform::from_translation(i.position)));
        }
        }
    }
}

// --- Ball Spawning (Including User's Preferred Color and Placement Fix) ---

fn ball_color(n: usize) -> Color {
    match n {
        1 | 9 => Color::rgb(1.0, 1.0, 0.0), // Yellow (1 = solid, 9 = stripe)
        2     => Color::rgb(0.0, 0.0, 1.0), // Blue
        3     => Color::rgb(1.0, 0.0, 0.0), // Red
        4     => Color::rgb(0.5, 0.0, 0.5), // Purple
        5     => Color::rgb(1.0, 0.5, 0.0), // Orange
        6     => Color::rgb(0.0, 1.0, 0.0), // Green
        7     => Color::rgb(0.5, 0.0, 0.0), // Maroon
        8     => Color::BLACK,             // Black
        _     => Color::WHITE,             // Fallback
    }
}

fn spawn_pool_balls(mut commands: Commands, mut meshes: ResMut<Assets<Mesh>>,   mut materials: ResMut<Assets<StandardMaterial>>, mut fonts: ResMut<Assets<Font>>, gamestate: Res<GameState>) {

    // Spawn Cue Ball with correct height
    commands.spawn(CueBall).insert(MaterialMeshBundle {
        mesh: meshes.add(Sphere::new(CUE_BALL_RADIUS)), 
        material: materials.add(StandardMaterial::from_color(WHITE)), 
        ..default()
    }).insert(Collider::ball(CUE_BALL_RADIUS)).insert(Sensor).insert(Transform::from_translation(Vec3::from((0.0,CUE_BALL_RADIUS, 0.0))));
    
    // Spawn Object Balls (1-9) with correct color and height
    for i in 1 as usize..=9 as usize{
        let color = ball_color(i);
        commands.spawn(PoolBalls(i))
            .insert(MaterialMeshBundle {
                mesh: meshes.add(Sphere::new(STANDARD_BALL_RADIUS)), 
                material: materials.add(StandardMaterial::from_color(color)), 
                // FIX: Setting Y-translation to STANDARD_BALL_RADIUS to lift the ball off the table.
                transform: Transform::from_translation(Vec3::new(0.0, STANDARD_BALL_RADIUS, 0.0)),
                ..default()
            })
            .insert(Collider::ball(STANDARD_BALL_RADIUS));
    }
}

// --- Input/Aiming Logic ---

fn get_vec3_of_local_cursor_position_from_global(camera: &Camera, camera_transform: &GlobalTransform, window: &Window) -> std::result::Result<Vec3, ()> {
        let ground_transform = Transform::from_translation(Vec3::new(0.0,0.0,0.0));
        // check if the cursor is inside the window and get its position
       if  let Some(cursor_position) = window.cursor_position() {
        let plane_origin = Vec3::new(0.0,STANDARD_BALL_RADIUS,0.0);
        let plane = InfinitePlane3d::new(Vec3::Y);
        let Some(ray) = camera.viewport_to_world(camera_transform, cursor_position) else {
            // if it was impossible to compute for whatever reason; we can't do anything
            return Err(());
        };
    
       
         // do a ray-plane intersection test, giving us the distance to the ground
         let Some(distance) = ray.intersect_plane(plane_origin, plane) else {
            // If the ray does not intersect the ground
            // (the camera is not looking towards the ground), we can't do anything
            return Err(());
        };
         // use the distance to compute the actual point on the ground in world-space
         let global_cursor = ray.get_point(distance);
        // to compute the local coordinates, we need the inverse of the plane's transform
        let inverse_transform_matrix = ground_transform.compute_matrix().inverse();
        let local_cursor = inverse_transform_matrix.transform_point3(global_cursor);
        return Ok(local_cursor)
} else {
    return Err(())
}
}


fn despawn_aimer_polyline(mut commands: Commands, mut aimer_query: Query<Entity, With< Aimer>>, ball_reaction_angle_query:  Query<Entity, With<BallReactionVector>>, reaction_angle_query: Query<Entity, With<ContactAngleVisual>>, shot_power_query: Query<Entity, With<ShotPower>>,) {
    if let Ok(aimer) = aimer_query.get_single() {
        commands.entity(aimer).remove::<Collider>();
    }

    if let Ok(angle) = ball_reaction_angle_query.get_single() {
        commands.entity(angle).remove::<Collider>();
    }

    if let Ok(reaction_angle) = reaction_angle_query.get_single() {
        commands.entity(reaction_angle).remove::<Collider>();
    }

    if let Ok(shot_power) = shot_power_query.get_single() {
        commands.entity(shot_power).remove::<Collider>();
    }


}

fn ball_in_hand(mut network_client: ResMut<NetworkClient>,camera_query:  Query<(&Camera, &GlobalTransform), With<MyGameCamera>>,keys: Res<ButtonInput<KeyCode>>, q_window: Query<&Window, With<PrimaryWindow>> ) {
    
     
        let window = q_window.single();
        let (camera, camera_transform) = camera_query.single();
        if let Ok(mut local_cursor) = get_vec3_of_local_cursor_position_from_global(camera, camera_transform, window) {
        if keys.just_pressed(KeyCode::KeyA) {
            let message = ClientMessage::BallPlacement { position: local_cursor };
            
            let payload = bincode::serialize(&message).unwrap();
            let ws_message = WsMessage::Binary(payload);
            let _ = network_client.sender.send(ws_message);

        }
        } 
     
      /* Create the cue ball. */

    

}





fn calculate_z(x: f32, y: f32, r: f32) -> Option<f32> {
    let z_squared = r.powi(2) - x.powi(2) - y.powi(2);
    if z_squared >= 0.0 {
        Some(z_squared.sqrt())
    } else {
        None // Return None if the point (x, y) is outside the sphere
    }
}

fn aim_system(mut network_client: ResMut<NetworkClient>,shot_power_query: Query<(Entity, &ShotPower)>,cue_ball_query: Query<(&Transform, Entity), With<CueBall>> , pool_ball_query: Query<(Entity, &Transform),With<PoolBalls>>, mut ball_reaction_angle_query:  Query<Entity, With<BallReactionVector>>, reaction_angle_query: Query<Entity, With<ContactAngleVisual>>, rapier_context: Res<RapierContext>, mut commands: Commands, keys: Res<ButtonInput<KeyCode>>,  camera_query:  Query<(&Camera, &GlobalTransform), With<MyGameCamera>>,  q_window: Query<&Window, With<PrimaryWindow>>, mut aimer_query: Query<Entity, With< Aimer>>) {
    // There is only one primary window, so we can similarly get it from the query:

    let (shot_power_entity, shot_power) = shot_power_query.single();
    let window = q_window.single();
    let ground_transform = Transform::from_translation(Vec3::new(0.0,0.0,0.0));
     let (camera, camera_transform) = camera_query.single();
    let plane_origin = Vec3::new(0.0,STANDARD_BALL_RADIUS,0.0);
    let plane = InfinitePlane3d::new(Vec3::Y);
    let Some(cursor_position) = window.cursor_position() else {
        // if the cursor is not inside the window, we can't do anything
        return;
    };

    let Some(ray) = camera.viewport_to_world(camera_transform, cursor_position) else {
        // if it was impossible to compute for whatever reason; we can't do anything
        println!("impossible to compute for some reason");
        return;
    };


     // do a ray-plane intersection test, giving us the distance to the ground
     let Some(distance) = ray.intersect_plane(plane_origin, plane) else {
        // If the ray does not intersect the ground
        // (the camera is not looking towards the ground), we can't do anything
        return;
    };


     // use the distance to compute the actual point on the ground in world-space
     let global_cursor = ray.get_point(distance);
    // to compute the local coordinates, we need the inverse of the plane's transform
    let inverse_transform_matrix = ground_transform.compute_matrix().inverse();
    // check if the cursor is inside the window and get its position
    let local_cursor = inverse_transform_matrix.transform_point3(global_cursor);

    if let Ok((cue_ball, cue_ball_entity)) =  cue_ball_query.get_single() {
        println!("cue ball found");
        if let Ok( aimer) = aimer_query.get_single_mut(){

            
                let direction_of_aimer =  Vec3::new(local_cursor.x, STANDARD_BALL_RADIUS, local_cursor.z) -  cue_ball.translation ;
                
                    
                if let Ok(reaction_angle) = reaction_angle_query.get_single() {
            if let Some((collider_entity, shape_hit)) =  rapier_context.cast_shape(cue_ball.translation, Quat::default(), direction_of_aimer, &Collider::ball(STANDARD_BALL_RADIUS), ShapeCastOptions{compute_impact_geometry_on_penetration: false, max_time_of_impact: f32::MAX, stop_at_penetration: false, target_distance: 0.0}, QueryFilter::new().exclude_rigid_body(cue_ball_entity).exclude_sensors()) {//.cast_ray_and_get_normal(cue_ball.translation, direction_of_aimer,f32::MAX, false, QueryFilter::new().exclude_rigid_body(cue_ball_entity).exclude_sensors() ) {
                if let Some(details) = shape_hit.details {
                    let end_point = details.witness1;
                    if let Ok(ball_reaction_vector_entity) = ball_reaction_angle_query.get_single() {
                        if let Ok((contact_ball, contact_ball_transform)) = pool_ball_query.get(collider_entity) {

                            let second_end_point = contact_ball_transform.translation;
                            let ball_reaction_vector =  details.normal1;
                            commands.entity(ball_reaction_vector_entity)
                            .insert(Collider::polyline(vec![second_end_point, second_end_point - ball_reaction_vector.normalize_or_zero() * 2.0] , Some(vec![[0,1]])));
                        
                        }    
                         else {
                            if let Ok(ball_reaction_vector_entity) = ball_reaction_angle_query.get_single() {
                                commands.entity(ball_reaction_vector_entity)
                                .insert(Collider::polyline(vec![cue_ball.translation, cue_ball.translation - direction_of_aimer.normalize_or_zero() * 1.25], Some(vec![[0,1]])));
                                }
                        } 
                    
                    } 
                    commands.entity(aimer)
                    .insert(Collider::polyline(vec![cue_ball.translation, end_point ], Some(vec![[0,1]])));
                let cue_reaction_vector =   ((details.normal2.normalize_or_zero()  -  direction_of_aimer.normalize_or_zero()) * direction_of_aimer.length());
                commands.entity(reaction_angle)
                .insert(Collider::polyline(vec![end_point - CUE_BALL_RADIUS * direction_of_aimer, end_point - cue_reaction_vector * 0.25] , Some(vec![[0,1]])));
            
            }
            }
        else {
                    commands.entity(aimer)
                    .insert(Collider::polyline(vec![cue_ball.translation, cue_ball.translation + direction_of_aimer.normalize_or_zero() * 1.25 ], Some(vec![[0,1]])));
                commands.entity(reaction_angle)
                .insert(Collider::polyline(vec![local_cursor, local_cursor] , Some(vec![[0,1]])));
            if let Ok(ball_reaction_vector_entity) = ball_reaction_angle_query.get_single() {
                                commands.entity(ball_reaction_vector_entity)
                                .insert(Collider::polyline(vec![local_cursor, local_cursor], Some(vec![[0,1]])));
                                }
        } 
            }
        
        }
            
                    if keys.just_pressed(KeyCode::Space) {
                        let local_cursor_pos = Vec3::new(local_cursor.x, STANDARD_BALL_RADIUS, local_cursor.z);
                        let mut direction_vector = local_cursor_pos - cue_ball.translation;
                        direction_vector.y = 0.0;
                        let original_end_point = cue_ball.translation;
                        let desired_length = 0.02;
                        let original_length = original_end_point.distance(Vec3::new(local_cursor.x,  cue_ball.translation.y, local_cursor.z));
                        let new_point = original_end_point - (desired_length / original_length) * direction_vector;
                        let linvel_direction_vector = direction_vector.normalize_or_zero() * shot_power.0 ;
                        let message = ClientMessage::Shot{ 
                              power: shot_power.0 * 1.25, 
                              direction: direction_vector.normalize_or_zero(), 
                              angvel: Vec3::ZERO 
                          };
                          let payload = bincode::serialize(&message).unwrap();
                          let ws_message = WsMessage::Binary(payload);
                          let _ = network_client.sender.send(ws_message);
                    }
    
        }

}





// --- Visual/Cosmetic Logic ---

fn rotate_torus(mut query: Query<&mut Transform, With<TargetBallTorus>>, time: Res<Time>) {
    let mut counter = 1.0;
    for mut transform in &mut query {
        transform.rotate_y(time.delta_seconds() / 0.25);
        transform.rotate_x(time.delta_seconds() / 0.25);
        transform.rotate_z(time.delta_seconds() / 0.25);
    }
}

fn rotate_numbers_around_pool_balls(mut commands: Commands, mut number_query: Query<&mut Transform, With<FloatingNumber>>, time: Res<Time> ) {
    let mut counter = 1.0;
    for mut transform in &mut number_query {
        transform.rotate_y(time.delta_seconds() / 0.25 * counter);
        //transform.rotate_x(time.delta_seconds() / 0.25 * counter);
        //transform.rotate_z(time.delta_seconds() / 0.25 * counter);
        counter *= -1.0;
        counter += 0.2 
    }
}

fn show_numbers_above_pool_balls(mut commands: Commands, mut ball_query: Query<(Entity, &Transform, &PoolBalls)>, mut floater_query: Query<(&mut Transform, &FloatingNumber), Without<PoolBalls>>) {
    for (pool_ball_entity,  pool_ball_transform, pool_ball_itself) in ball_query.iter_mut() {
        for  (mut floater_transform, number_itself) in floater_query.iter_mut() {
            if number_itself.0  == pool_ball_itself.0 {
                floater_transform.translation = pool_ball_transform.translation + Vec3::Y * 0.04;
                floater_transform.align(-Dir3::Y, Dir3::X, -Dir3::X, Dir3::Z);
            }
        }
    }
}

fn setup_numbers_above_pool_balls( mut commands: Commands, mut meshes: ResMut<Assets<Mesh>>, mut materials: ResMut<Assets<StandardMaterial>>, ball_query: Query<(Entity, &Transform, &PoolBalls)>) {
    let font_data = include_bytes!("../../assets/fonts/Roboto-Black.ttf");
    let mut generator = MeshGenerator::new(font_data);
    for (pool_ball_entity, pool_ball_transform, pool_ball_itself) in ball_query.iter() {
        let translation = pool_ball_transform.translation;
        let raw_number = pool_ball_itself.0;
        let number = raw_number.to_string().as_str().to_owned();
    let transform = Mat4::from_scale(Vec3::new(0.075, 0.075, 0.0075)).to_cols_array();
    let text_mesh: MeshText = generator
        .generate_section(&number, false, Some(&transform))
        .unwrap();

    let vertices = text_mesh.vertices;
    let positions: Vec<[f32; 3]> = vertices.chunks(3).map(|c| [c[0], c[1], c[2]]).collect();
    let uvs = vec![[0f32, 0f32]; positions.len()];
    let mut mesh = Mesh::new(bevy::render::render_resource::PrimitiveTopology::TriangleList, RenderAssetUsages::RENDER_WORLD);
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uvs);
    mesh.compute_flat_normals();
    let material = match raw_number {
        _ => StandardMaterial::from_color(Color::linear_rgb(0.60, 0.80, 1.0))
    };
    commands.spawn(FloatingNumber(raw_number as usize)).insert(MaterialMeshBundle {mesh: meshes.add(mesh), material: materials.add(material), transform: Transform {translation: Vec3::new(0.0,  0.00, 0.0), rotation: Quat::from_rotation_x(0.0), scale: Vec3::ONE * 1.5},visibility: Visibility::Visible,..default()});
    }
}


fn increase_shot_power(mut commands: Commands,  mut shot_power_query: Query<(Entity, &mut ShotPower)>) {
    if let Ok((shot_power_entity, mut shot_power)) = shot_power_query.get_single_mut() {
        if shot_power.1 == true {
            shot_power.0 += 0.02;
        } else {
            shot_power.0 -= 0.02
        }
        if shot_power.0 >= 4.5 {
            shot_power.1 = false;
        }
        if shot_power.0 <= 2.0 {
            shot_power.1 = true;
        }
        if shot_power.0 >= 2.50 && shot_power.1 == true {
            //increase faster
            shot_power.0 += 0.02;
        }
        if shot_power.0 >= 2.75 && shot_power.1 == true {
            //increase faster
            shot_power.0 += 0.02;
        }
        if shot_power.0 >= 2.50 && shot_power.1 == false {
            //increase faster
            shot_power.0 -= 0.02;
        }
        if shot_power.0 >= 2.75 && shot_power.1 == false {
            //increase faster
            shot_power.0 -= 0.02;
        }
    }
}

fn display_shot_power(mut commands: Commands, mut meshes: ResMut<Assets<Mesh>>, mut materials: ResMut<Assets<StandardMaterial>>, shot_power_query: Query<(Entity, &ShotPower)>, camera_query:  Query<(&Camera, &GlobalTransform), With<MyGameCamera>>, gamestate: Res<GameState>, aimer_query: Query<&Transform, With<Aimer>>, q_window: Query<&Window, With<PrimaryWindow>>) {
    
    if let Some(cue_ball_data) = gamestate.balls.iter().find(|b| b.is_cue){
        let cue_ball_transform = Transform::from_translation(cue_ball_data.position);
    
        let window = q_window.single();
        let ground_transform = Transform::from_translation(Vec3::new(0.0,0.0,0.0));
        // check if the cursor is inside the window and get its position
        let Some(cursor_position) = window.cursor_position() else {
            // if the cursor is not inside the window, we can't do anything
            return;
        };
    
        let (camera, camera_transform) = camera_query.single();
        let plane_origin = Vec3::new(0.0,STANDARD_BALL_RADIUS,0.0);
        let plane = InfinitePlane3d::new(Vec3::Y);
        let Some(ray) = camera.viewport_to_world(camera_transform, cursor_position) else {
            // if it was impossible to compute for whatever reason; we can't do anything
            return;
        };
    
    
         // do a ray-plane intersection test, giving us the distance to the ground
         let Some(distance) = ray.intersect_plane(plane_origin, plane) else {
            // If the ray does not intersect the ground
            // (the camera is not looking towards the ground), we can't do anything
            return;
        };
         // use the distance to compute the actual point on the ground in world-space
         let global_cursor = ray.get_point(distance);
        // to compute the local coordinates, we need the inverse of the plane's transform
        let inverse_transform_matrix = ground_transform.compute_matrix().inverse();
        let local_cursor = inverse_transform_matrix.transform_point3(global_cursor);
        let end_point_one =     Vec3::new(local_cursor.x, STANDARD_BALL_RADIUS, local_cursor.z) ;
        let end_point_two = cue_ball_transform.translation;
        if let Ok((shot_power_entity, shot_power_itself)) = shot_power_query.get_single() {
            let mut direction = (  end_point_two - end_point_one ).normalize_or_zero();
            direction.y = 0.0;
            let intermediary = -direction * -(shot_power_itself.0 + 0.2);
            let original_length = 1.0;
            let projected_point = end_point_two - ((shot_power_itself.0+ 0.002) / original_length) * -direction;
            commands.entity(shot_power_entity).insert(Collider::polyline(vec![projected_point, cue_ball_transform.translation] , Some(vec![[0,1]])));
        }

    };
}