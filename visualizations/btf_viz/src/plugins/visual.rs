use bevy::prelude::*;
use bevy::sprite::Sprite;

use crate::model::state::{Grid, Trace};

#[derive(Resource)]
pub struct Layout {
    pub matrix_origin: Vec3,
    pub matrix_cell: Vec2,
    pub arrays_cell: Vec2,
    pub gap: f32,
}

impl Default for Layout {
    fn default() -> Self {
        Self {
            matrix_origin: Vec3::new(0.0, 0.0, 0.0),
            matrix_cell: Vec2::new(40.0, 40.0),
            arrays_cell: Vec2::new(36.0, 36.0),
            gap: 24.0,
        }
    }
}

#[derive(Component)]
struct MatrixCell {
    idx: usize,
}

#[derive(Component)]
struct CellText;

#[derive(Component)]
struct BracketEntity;

pub struct VisualPlugin;

impl Plugin for VisualPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Layout>().add_systems(
            Update,
            (
                ensure_grid_spawned,
                refresh_grid,
                refresh_arrays,
                animate_highlights,
            ),
        );
    }
}

fn spawn_cell_with_text(
    commands: &mut Commands,
    pos: Vec3,
    size: Vec2,
    idx: usize,
    value: Option<f64>,
) {
    let bg_color = if value.is_some() {
        Color::WHITE
    } else {
        Color::srgba(1.0, 1.0, 1.0, 0.08)
    };
    let text_string = value.map(|v| format!("{}", v)).unwrap_or_else(String::new);

    commands
        .spawn((
            Sprite {
                color: bg_color,
                custom_size: Some(Vec2::new(size.x - 6.0, size.y - 6.0)),
                ..default()
            },
            Transform::from_translation(pos),
            Visibility::default(),
            MatrixCell { idx },
        ))
        .with_children(|p| {
            p.spawn((
                Text2d::new(text_string),
                TextFont::from(Handle::<Font>::default()).with_font_size(size.y * 0.55),
                TextColor(Color::BLACK),
                Transform::from_translation(Vec3::new(0.0, 0.0, 0.1)),
                CellText,
            ));
        });
}

#[derive(Component)]
struct ArrayCell {
    name: &'static str,
    idx: usize,
}

#[derive(Component)]
struct ArrayRoot {
    name: &'static str,
}

fn spawn_array_view(
    commands: &mut Commands,
    center: Vec3,
    cell: Vec2,
    name: &'static str,
    values: &[String],
) {
    let len = values.len();
    if len == 0 {
        return;
    }
    let total_w = len as f32 * cell.x;
    let left_x = center.x - total_w / 2.0 + cell.x / 2.0;

    // Label
    commands.spawn((
        Text2d::new(name.to_string()),
        TextFont::from(Handle::<Font>::default()).with_font_size(cell.y * 0.45),
        TextColor(Color::WHITE),
        Transform::from_translation(Vec3::new(center.x, center.y + cell.y, center.z)),
        ArrayRoot { name },
    ));

    for i in 0..len {
        let x = left_x + i as f32 * cell.x;
        let value = values[i].clone();
        commands
            .spawn((
                Sprite {
                    color: Color::srgba(1.0, 1.0, 1.0, 0.12),
                    custom_size: Some(Vec2::new(cell.x - 6.0, cell.y - 6.0)),
                    ..default()
                },
                Transform::from_translation(Vec3::new(x, center.y, center.z)),
                Visibility::default(),
                ArrayCell { name, idx: i },
            ))
            .with_children(|p| {
                p.spawn((
                    Text2d::new(value),
                    TextFont::from(Handle::<Font>::default()).with_font_size(cell.y * 0.55),
                    TextColor(Color::WHITE),
                    CellText,
                ));
            });
    }
}

fn ensure_grid_spawned(
    mut commands: Commands,
    grid: Res<Grid>,
    existing: Query<Entity, With<MatrixCell>>,
    layout: Res<Layout>,
    trace: Res<Trace>,
) {
    if !existing.is_empty() || grid.rows == 0 || grid.cols == 0 {
        return;
    }
    let cell_w = layout.matrix_cell.x;
    let cell_h = layout.matrix_cell.y;
    let w = grid.cols as f32 * cell_w;
    let h = grid.rows as f32 * cell_h;
    let origin = Vec3::new(
        layout.matrix_origin.x - w / 2.0 + cell_w / 2.0,
        layout.matrix_origin.y + h / 2.0 - cell_h / 2.0,
        layout.matrix_origin.z,
    );

    for r in 0..grid.rows {
        for c in 0..grid.cols {
            let idx = r * grid.cols + c;
            let x = origin.x + c as f32 * cell_w;
            let y = origin.y - r as f32 * cell_h;
            let value = grid.values.get(idx).and_then(|v| *v);
            spawn_cell_with_text(
                &mut commands,
                Vec3::new(x, y, 0.0),
                Vec2::new(cell_w, cell_h),
                idx,
                value,
            );
        }
    }

    // Brackets (simple rectangles as placeholders)
    let thick = 6.0f32;
    let cap = 50.0f32;
    let left_x = layout.matrix_origin.x - w / 2.0 - thick;
    let right_x = layout.matrix_origin.x + w / 2.0 + thick;
    let z = -0.1;
    // Left vertical
    commands.spawn((
        Sprite {
            color: Color::WHITE,
            custom_size: Some(Vec2::new(thick, h + cap * 2.0)),
            ..default()
        },
        Transform::from_translation(Vec3::new(left_x, layout.matrix_origin.y, z)),
        Visibility::default(),
        BracketEntity,
    ));
    // Right vertical
    commands.spawn((
        Sprite {
            color: Color::WHITE,
            custom_size: Some(Vec2::new(thick, h + cap * 2.0)),
            ..default()
        },
        Transform::from_translation(Vec3::new(right_x, layout.matrix_origin.y, z)),
        Visibility::default(),
        BracketEntity,
    ));
    // Top caps
    commands.spawn((
        Sprite {
            color: Color::WHITE,
            custom_size: Some(Vec2::new(cap, thick)),
            ..default()
        },
        Transform::from_translation(Vec3::new(
            left_x + cap / 2.0,
            layout.matrix_origin.y + h / 2.0 + cap,
            z,
        )),
        Visibility::default(),
        BracketEntity,
    ));
    commands.spawn((
        Sprite {
            color: Color::WHITE,
            custom_size: Some(Vec2::new(cap, thick)),
            ..default()
        },
        Transform::from_translation(Vec3::new(
            right_x - cap / 2.0,
            layout.matrix_origin.y + h / 2.0 + cap,
            z,
        )),
        Visibility::default(),
        BracketEntity,
    ));
    // Bottom caps
    commands.spawn((
        Sprite {
            color: Color::WHITE,
            custom_size: Some(Vec2::new(cap, thick)),
            ..default()
        },
        Transform::from_translation(Vec3::new(
            left_x + cap / 2.0,
            layout.matrix_origin.y - h / 2.0 - cap,
            z,
        )),
        Visibility::default(),
        BracketEntity,
    ));
    commands.spawn((
        Sprite {
            color: Color::WHITE,
            custom_size: Some(Vec2::new(cap, thick)),
            ..default()
        },
        Transform::from_translation(Vec3::new(
            right_x - cap / 2.0,
            layout.matrix_origin.y - h / 2.0 - cap,
            z,
        )),
        Visibility::default(),
        BracketEntity,
    ));

    // Arrays
    let arr_cell = layout.arrays_cell;
    // column_permutations above matrix
    let colperm: Vec<String> = trace
        .column_permutations
        .iter()
        .map(|v| format!("{}", v))
        .collect();
    let cp_center = Vec3::new(
        layout.matrix_origin.x,
        layout.matrix_origin.y + h / 2.0 + layout.gap + arr_cell.y,
        0.0,
    );
    spawn_array_view(
        &mut commands,
        cp_center,
        arr_cell,
        "column_permutations",
        &colperm,
    );

    // Right-hand arrays stacked
    let right_x_center = layout.matrix_origin.x + w / 2.0 + layout.gap + (arr_cell.x * 3.0);
    let mut y = layout.matrix_origin.y + h / 2.0 - arr_cell.y * 0.5;
    let vspace = arr_cell.y + layout.gap * 0.75;

    let column_stack_vals: Vec<String> = trace
        .column_stack
        .iter()
        .map(|v| format!("{}", v))
        .collect();
    spawn_array_view(
        &mut commands,
        Vec3::new(right_x_center, y, 0.0),
        arr_cell,
        "column_stack",
        &column_stack_vals,
    );
    y -= vspace;

    let row_stack_vals: Vec<String> = trace.row_stack.iter().map(|v| format!("{}", v)).collect();
    spawn_array_view(
        &mut commands,
        Vec3::new(right_x_center, y, 0.0),
        arr_cell,
        "row_stack",
        &row_stack_vals,
    );
    y -= vspace;

    let pstack_vals: Vec<String> = trace
        .position_stack
        .iter()
        .map(|v| format!("{}", v))
        .collect();
    spawn_array_view(
        &mut commands,
        Vec3::new(right_x_center, y, 0.0),
        arr_cell,
        "position_stack",
        &pstack_vals,
    );
    y -= vspace;

    let visited_vals: Vec<String> = trace.visited.iter().map(|v| format!("{}", v)).collect();
    spawn_array_view(
        &mut commands,
        Vec3::new(right_x_center, y, 0.0),
        arr_cell,
        "visited",
        &visited_vals,
    );
    y -= vspace;

    let cheap_vals: Vec<String> = trace.cheap.iter().map(|v| format!("{}", v)).collect();
    spawn_array_view(
        &mut commands,
        Vec3::new(right_x_center, y, 0.0),
        arr_cell,
        "cheap",
        &cheap_vals,
    );
}

fn refresh_grid(
    mut q_cells: Query<(&MatrixCell, &mut Sprite, &Children)>,
    mut q_text: Query<&mut Text2d, With<CellText>>,
    grid: Res<Grid>,
) {
    if !grid.is_changed() {
        return;
    }

    for (ce, mut sprite, children) in q_cells.iter_mut() {
        let idx = ce.idx;
        let value = grid.values.get(idx).and_then(|v| *v);
        sprite.color = if value.is_some() {
            Color::WHITE
        } else {
            Color::srgba(1.0, 1.0, 1.0, 0.08)
        };

        let new_value = value.map(|v| format!("{}", v)).unwrap_or_else(String::new);
        for i in 0..children.len() {
            let child = children[i];
            if let Ok(mut text) = q_text.get_mut(child) {
                text.0 = new_value.clone();
            }
        }
    }
}

fn animate_highlights(_time: Res<Time>, _grid: Res<Grid>, _q: Query<(Entity,)>) {
    // Placeholder: color/scale anims for highlights
}

fn refresh_arrays(
    trace: Res<Trace>,
    mut q_cells: Query<(&ArrayCell, &Children)>,
    mut q_text: Query<&mut Text2d, With<CellText>>,
) {
    if !trace.is_changed() {
        return;
    }

    for (cell, children) in q_cells.iter_mut() {
        let value_opt = match cell.name {
            "column_permutations" => trace
                .column_permutations
                .get(cell.idx)
                .map(|v| format!("{}", v)),
            "column_stack" => trace.column_stack.get(cell.idx).map(|v| format!("{}", v)),
            "row_stack" => trace.row_stack.get(cell.idx).map(|v| format!("{}", v)),
            "position_stack" => trace.position_stack.get(cell.idx).map(|v| format!("{}", v)),
            "visited" => trace.visited.get(cell.idx).map(|v| format!("{}", v)),
            "cheap" => trace.cheap.get(cell.idx).map(|v| format!("{}", v)),
            _ => None,
        };

        let value = value_opt.unwrap_or_else(|| String::new());
        for i in 0..children.len() {
            let child = children[i];
            if let Ok(mut text) = q_text.get_mut(child) {
                text.0 = value.clone();
            }
        }
    }
}
