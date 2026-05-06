#### https://nine-ball.net

### Purpose of Project 
This project features the majority of the source code supporting the implementation of nine-ball.net, a website devoted to 2-player digital nine-ball pool.

The actual browser-based game, written in Rust, has been compiled to WebAssembly (WASM) for more universal online access to play.

After registration, match challenges can be issued to the lobby where other players can take up the challenge.

### In-game controls: 
  Trigger a shot with the spacebar.
  The mouse pointer aims the direction of the shot; the ball will move towards(!) the location of the cursor on the table.
    When the cue ball is set to collide with an object ball, two reaction vectors will appear: the direction vectors of the object ball and cue ball at the instantaneous point of contact.
  Place the cue ball after a scratch (after all object balls have ceased their motion) with the A-key.

### Note
As of 5-6-26:
  No rating algorithm has been implemented. Everyone starts at 1500 elo, and as of now, that value is unchanging.
  When a game ends, a screen saying: "Other Player Disconnected" will display. No one is declared victorious in terms of the UI.
  As such, player match history is not currently viewable by users.
  Due to WASM-based windowing contraints, the application of spin is unimplimented in this application.
  Correspondence 9-ball (a proposed future-state mode where players have a longer time period to play their turns) is currently unimplemented.

## Project Structure

  ### nine_ball_game
    #### contains game logic for the WASM-compiled Bevy project.
  ### ninballnet
    #### contains code related to the Loco.rs (a Rails-like Rust framework) application handling frontend logic and db query logic.
  ### ninballnetallocator
    #### this codebase contains logic related to the matchmaking functionality of nine-ball.net.
