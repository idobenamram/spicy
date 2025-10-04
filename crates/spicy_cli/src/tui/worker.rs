use std::path::PathBuf;

use crossbeam_channel::{Receiver, Sender};
use spicy_simulate::{
    DcSweepResult, OperatingPointResult, TransientResult,
    dc::{simulate_dc, simulate_op},
    trans::simulate_trans,
};

use crate::tui::app::{App, Tab};
use spicy_parser::{error::SpicyError, netlist_types::Command, parse, ParseOptions};

#[derive(Clone, Debug)]
pub enum SimCmd {
    RunCurrentTab(Tab),
}

#[derive(Debug)]
pub enum SimMsg {
    Diagnostics(Vec<SpicyError>),
    SimulationStarted,
    Op(OperatingPointResult),
    Dc(DcSweepResult),
    Transient(TransientResult),
    Done,
}

pub fn apply_sim_update(app: &mut App, msg: SimMsg) {
    match msg {
        SimMsg::Diagnostics(d) => app.diags = d,
        SimMsg::SimulationStarted => app.running = true,
        SimMsg::Op(op) => app.op = Some(op),
        SimMsg::Dc(dc) => app.dc = Some(dc),
        SimMsg::Transient(tr) => app.trans = Some(tr),
        SimMsg::Done => app.running = false,
    }
}

pub fn worker_loop(netlist: String, rx: Receiver<SimCmd>, tx: Sender<SimMsg>) {
    while let Ok(cmd) = rx.recv() {
        match cmd {
            SimCmd::RunCurrentTab(_tab) => {
                // Parse
                // TODO: fix
                let parse_options = ParseOptions {
                    work_dir: PathBuf::from(""),
                    source_path: PathBuf::from(""),
                    input: &netlist,
                };

                let deck = match parse(&parse_options) {
                    Ok(deck) => deck,
                    Err(e) => {
                        let mut diags = Vec::new();
                        diags.push(e);
                        let _ = tx.send(SimMsg::Diagnostics(diags));
                        continue;
                    }
                };

                for command in &deck.commands {
                    match command {
                        Command::Op(_) => {
                            let op = simulate_op(&deck);
                            let _ = tx.send(SimMsg::Op(op));
                            continue;
                        }
                        Command::Dc(command_params) => {
                            let dc = simulate_dc(&deck, &command_params);
                            let _ = tx.send(SimMsg::Dc(dc));
                            continue;
                        }
                        Command::Tran(command_params) => {
                            let tr = simulate_trans(&deck, &command_params);
                            let _ = tx.send(SimMsg::Transient(tr));
                            continue;
                        }
                        _ => {}
                    }
                }

                let _ = tx.send(SimMsg::Done);
            }
        }
    }
}

fn offset_to_line(src: &str, byte_offset: usize) -> usize {
    let offset = byte_offset.min(src.len());
    let prefix = &src[..offset];
    prefix.chars().filter(|&c| c == '\n').count() + 1
}
