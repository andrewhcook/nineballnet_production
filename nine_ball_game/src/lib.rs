

use std::default;
// In src/lib.rs
// This allows the compiler to see the contents of src/client/
// src/lib.rs
use bevy::prelude::*;
use bevy_rapier3d::prelude::{Damping, Restitution, Velocity};
use serde::{Deserialize, Serialize};

// --- Physics Constants ---
pub const TABLE_LENGTH: f32  = 1.3716 * 1.05;
pub const TABLE_WIDTH: f32  = TABLE_LENGTH / 2.0;
pub const STANDARD_BALL_RADIUS: f32 = 5.7 / 100.0 / 2.0;
pub const CUE_BALL_RADIUS: f32 = 5.7127 / 100.0 / 2.0;
pub const BALL_MASS: f32 = 0.17;
pub const TABLE_FRICTION_COEFF: f32 = 1.00;
pub const FRICTION_COEFF: f32 = 1.0;
pub const BALL_FRICTION_COEFF:f32 = 1.0;
pub const BALL_RESTITUTION: Restitution = Restitution::coefficient(1.00);
pub const DEFAULT_VELOCITY: Velocity =  Velocity {
    linvel: Vec3::ZERO,
    angvel: Vec3::ZERO

};
pub const BALL_DAMPING: Damping = Damping {
linear_damping: 0.2533301,
angular_damping: 0.253301
};
pub const WALL_DIMENSIONS: Cuboid = Cuboid{half_size: Vec3::new(0.02, 120.55, 0.6096)};
pub const WALL_MESH_DIMENSIONS: Cuboid = Cuboid{half_size: Vec3::new(0.02, 5.55, 0.6096)};
pub const BACK_WALL_MESH_DIMENSIONS: Cuboid = Cuboid{half_size: Vec3::new(0.6096, 120.55, 0.002)};
pub const TARGET_BALL_TORUS_DIMENSIONS: Torus = Torus{ minor_radius: 0.002 , major_radius: 0.06 };
pub const CAMERA_HEIGHT: Vec3 = Vec3 {x: 0.0, y: 1.97, z: 0.0};

// --- Shared Data Protocol ---
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct BallData {
    pub number: usize,
    pub position: Vec3,
    pub velocity: Vec3,
    pub rotation: Quat,
    pub is_cue: bool,
}
// ... (rest of GameState, GamePhase, ClientMessage remain the same)
#[derive(Resource, Serialize, Deserialize, Debug, Clone, Default)]
pub struct GameState {
    pub balls: Vec<BallData>,
    pub phase: GamePhase,
    pub should_show_shot_controls: bool,
    pub whose_move: WhoseMove
}


#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum ClientMessage {
    Join { name: String },
    Shot { power: f32, direction: Vec3, angvel: Vec3 }, 
    BallPlacement { position: Vec3 },
}

#[derive(States,Default, Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum WhoseMove {
    #[default]
    Player1,
    Player2
}



#[derive(States, Default,Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum GamePhase {
    #[default]
    PreShot,
    BallInHand,
    InMotion,
    PostShot
}
