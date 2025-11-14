use bevy::prelude::*;
use bevy_egui::egui;

use crate::model::state::{Player, Trace};

#[derive(Resource, Default)]
struct CodePanelVisible(bool);

pub struct UiPlugin;

impl Plugin for UiPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<CodePanelVisible>()
            .add_systems(bevy_egui::EguiPrimaryContextPass, toggle_code_panel)
            .add_systems(bevy_egui::EguiPrimaryContextPass, ui_system);
    }
}

fn toggle_code_panel(kb: Res<ButtonInput<KeyCode>>, mut visible: ResMut<CodePanelVisible>) {
    if kb.just_pressed(KeyCode::KeyC) {
        visible.0 = !visible.0;
    }
}

fn ui_system(
    mut egui_ctx: bevy_egui::EguiContexts,
    mut player: ResMut<Player>,
    mut trace: ResMut<Trace>,
    visible: Res<CodePanelVisible>,
) {
    if let Ok(ctx) = egui_ctx.ctx_mut() {
        egui::SidePanel::left("controls").show(ctx, |ui| {
            ui.heading("BTF Max Transversal");

            ui.horizontal(|ui| {
                if ui
                    .button(if player.paused { "Play" } else { "Pause" })
                    .clicked()
                {
                    player.paused = !player.paused;
                }
                if ui.button("Step").clicked() {
                    trace.next();
                }
                if ui.button("Prev").clicked() {
                    trace.prev();
                }
            });

            ui.add(egui::Slider::new(&mut player.speed, 0.25..=8.0).text("Speed"));

            ui.separator();
            let step_count = trace.steps.len();
            let current_idx = trace.idx;
            ui.label(format!("Step {}/{}", current_idx + 1, step_count.max(1)));
            let mut idx = current_idx as u32;
            if ui
                .add(egui::Slider::new(
                    &mut idx,
                    0..=(step_count as u32).saturating_sub(1),
                ))
                .changed()
            {
                trace.apply_steps_to(idx as usize);
            }

            if visible.0 {
                ui.separator();
                ui.label("Code view");
                egui::ScrollArea::vertical().show(ui, |ui| {
                    let code_lines = trace.code_lines.clone();
                    let active_line = trace.active_code_line;
                    for (i, line) in code_lines.iter().enumerate() {
                        let line_num = (i + 1) as u32;
                        let active = line_num == active_line;
                        let txt = if active {
                            egui::RichText::new(format!("{:4} {}", line_num, line))
                                .background_color(egui::Color32::from_gray(32))
                        } else {
                            egui::RichText::new(format!("{:4} {}", line_num, line))
                        };
                        ui.label(txt.monospace());
                    }
                });
            }
        });
    }
}
