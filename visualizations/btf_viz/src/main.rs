use bevy::prelude::*;

mod code;
mod model;
mod plugins;

fn main() {
    App::new()
        .insert_resource(ClearColor(Color::BLACK))
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: "BTF Max Transversal Visualizer".into(),
                ..default()
            }),
            ..default()
        }))
        .add_plugins(bevy_egui::EguiPlugin::default())
        .add_plugins((
            plugins::runloop::RunloopPlugin,
            plugins::visual::VisualPlugin,
            plugins::ui::UiPlugin,
        ))
        .run();
}
