use bevy::prelude::*;
use bevy::sprite::Sprite;

use crate::model::state::Grid;

#[derive(Component)]
struct CellEntity {
    idx: usize,
}

#[derive(Component)]
struct BracketEntity;

pub struct VisualPlugin;

impl Plugin for VisualPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, (ensure_grid_spawned, refresh_grid, animate_highlights));
    }
}

fn ensure_grid_spawned(
    mut commands: Commands,
    grid: Res<Grid>,
    existing: Query<Entity, With<CellEntity>>,
) {
    if !existing.is_empty() || grid.rows == 0 || grid.cols == 0 { return; }
    let cell_w = 40.0f32;
    let cell_h = 40.0f32;
    let w = grid.cols as f32 * cell_w;
    let h = grid.rows as f32 * cell_h;
    let origin = Vec3::new(-w / 2.0 + cell_w / 2.0, h / 2.0 - cell_h / 2.0, 0.0);

    for r in 0..grid.rows {
        for c in 0..grid.cols {
            let idx = r * grid.cols + c;
            let x = origin.x + c as f32 * cell_w;
            let y = origin.y - r as f32 * cell_h;

            let color = if grid.nonzeros.get(idx).copied().unwrap_or(false) { Color::WHITE } else { Color::srgba(1.0, 1.0, 1.0, 0.08) };
            commands.spawn((
                Sprite { color, custom_size: Some(Vec2::new(cell_w - 6.0, cell_h - 6.0)), ..default() },
                Transform::from_translation(Vec3::new(x, y, 0.0)),
                Visibility::default(),
                CellEntity { idx },
            ));
        }
    }

    // Brackets (simple rectangles as placeholders)
    let thick = 6.0f32;
    let cap = 50.0f32;
    let left_x = -w / 2.0 - thick;
    let right_x = w / 2.0 + thick;
    let z = -0.1;
    // Left vertical
    commands.spawn((
        Sprite { color: Color::WHITE, custom_size: Some(Vec2::new(thick, h + cap * 2.0)), ..default() },
        Transform::from_translation(Vec3::new(left_x, 0.0, z)),
        Visibility::default(),
        BracketEntity,
    ));
    // Right vertical
    commands.spawn((
        Sprite { color: Color::WHITE, custom_size: Some(Vec2::new(thick, h + cap * 2.0)), ..default() },
        Transform::from_translation(Vec3::new(right_x, 0.0, z)),
        Visibility::default(),
        BracketEntity,
    ));
    // Top caps
    commands.spawn((
        Sprite { color: Color::WHITE, custom_size: Some(Vec2::new(cap, thick)), ..default() },
        Transform::from_translation(Vec3::new(left_x + cap / 2.0, h / 2.0 + cap, z)),
        Visibility::default(),
        BracketEntity,
    ));
    commands.spawn((
        Sprite { color: Color::WHITE, custom_size: Some(Vec2::new(cap, thick)), ..default() },
        Transform::from_translation(Vec3::new(right_x - cap / 2.0, h / 2.0 + cap, z)),
        Visibility::default(),
        BracketEntity,
    ));
    // Bottom caps
    commands.spawn((
        Sprite { color: Color::WHITE, custom_size: Some(Vec2::new(cap, thick)), ..default() },
        Transform::from_translation(Vec3::new(left_x + cap / 2.0, -h / 2.0 - cap, z)),
        Visibility::default(),
        BracketEntity,
    ));
    commands.spawn((
        Sprite { color: Color::WHITE, custom_size: Some(Vec2::new(cap, thick)), ..default() },
        Transform::from_translation(Vec3::new(right_x - cap / 2.0, -h / 2.0 - cap, z)),
        Visibility::default(),
        BracketEntity,
    ));
}

fn refresh_grid(mut q: Query<(&CellEntity, &mut Sprite)>, grid: Res<Grid>) {
    if !grid.is_changed() {
        return;
    }
    for (ce, mut sprite) in q.iter_mut() {
        let idx = ce.idx;
        let on = grid.nonzeros.get(idx).copied().unwrap_or(false);
        sprite.color = if on { Color::WHITE } else { Color::srgba(1.0, 1.0, 1.0, 0.08) };
    }
}

fn animate_highlights(_time: Res<Time>, _grid: Res<Grid>, _q: Query<(Entity,)>) {
    // Placeholder: color/scale anims for highlights
}


