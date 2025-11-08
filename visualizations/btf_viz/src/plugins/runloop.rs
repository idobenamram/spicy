use bevy::prelude::*;

use crate::model::state::{Grid, Player, Trace};
use crate::model::trace::TraceFile;

pub struct RunloopPlugin;

impl Plugin for RunloopPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Player>()
            .init_resource::<Grid>()
            .init_resource::<Trace>()
            .add_systems(Startup, (setup_camera, load_trace))
            .add_systems(Update, (hotkeys, tick_player, sync_grid_from_trace));
    }
}

fn setup_camera(mut commands: Commands) {
    commands.spawn(Camera2d::default());
}

fn load_trace(
    mut grid: ResMut<Grid>,
    mut trace: ResMut<Trace>,
) {
    // Try to load trace from file, or use default empty trace
    let trace_path = "assets/traces/sample_5x5.json";
    let code_text = include_str!("../code/btf_max_transversal.rs");
    
    match std::fs::read_to_string(trace_path) {
        Ok(trace_json) => {
            if let Ok(tf) = serde_json::from_str::<TraceFile>(&trace_json) {
                *trace = Trace::load_from(tf, code_text.to_string());
                
                // Initialize grid from trace (numeric values)
                let (rows, cols, values) = trace.extract_matrix_data();
                grid.rows = rows;
                grid.cols = cols;
                grid.values = values;
                grid.matching = trace.get_matching();
            } else {
                error!("Failed to parse trace file: {}", trace_path);
            }
        }
        Err(e) => {
            warn!("Could not load trace file {}: {}. Run 'cargo run --bin generate_trace' to generate it.", trace_path, e);
        }
    }
}

fn hotkeys(
    kb: Res<ButtonInput<KeyCode>>,
    mut trace: ResMut<Trace>,
    mut player: ResMut<Player>,
) {
    // Space: play/pause
    if kb.just_pressed(KeyCode::Space) {
        player.paused = !player.paused;
    }
    
    // Right Arrow: next step
    if kb.just_pressed(KeyCode::ArrowRight) {
        trace.next();
    }
    
    // Left Arrow: previous step
    if kb.just_pressed(KeyCode::ArrowLeft) {
        trace.prev();
    }
    
    // Up Arrow: increase speed
    if kb.just_pressed(KeyCode::ArrowUp) {
        player.speed = (player.speed * 1.5).min(8.0);
    }
    
    // Down Arrow: decrease speed
    if kb.just_pressed(KeyCode::ArrowDown) {
        player.speed = (player.speed / 1.5).max(0.25);
    }
}

fn tick_player(time: Res<Time>, mut player: ResMut<Player>, mut trace: ResMut<Trace>) {
    if player.paused {
        return;
    }
    player.accum += time.delta().as_secs_f32();
    let step_dt = (1.0 / player.speed).max(0.01);
    while player.accum >= step_dt {
        player.accum -= step_dt;
        trace.next();
    }
}

fn sync_grid_from_trace(
    trace: Res<Trace>,
    mut grid: ResMut<Grid>,
) {
    if !trace.is_changed() {
        return;
    }
    
    // Update matching from trace
    grid.matching = trace.get_matching();
}

