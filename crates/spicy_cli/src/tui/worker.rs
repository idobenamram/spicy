use std::path::PathBuf;

use crossbeam_channel::{Receiver, Sender};
use spicy_simulate::{
    DcSweepResult, OperatingPointResult, SimulationConfig, TransientResult,
    dc::{simulate_dc, simulate_op},
    trans::simulate_trans,
};

use crate::tui::app::{App, ResultNames};
use crate::tui::ui::format_error_snippet;
use spicy_circuit::Analysis;
use spicy_parser::{ParseOptions, SourceMap, error::SpicyError, lower, parse};

#[derive(Clone, Debug)]
pub enum SimCmd {
    RunCurrentTab { config: SimulationConfig },
}

#[derive(Debug)]
pub enum SimMsg {
    SimulationStarted(ResultNames),
    Op(OperatingPointResult),
    Dc(DcSweepResult),
    Transient(TransientResult),
    FatalError(String),
    Done,
}

pub fn apply_sim_update(app: &mut App, msg: SimMsg) {
    match msg {
        SimMsg::SimulationStarted(result_names) => {
            app.result_names = result_names;
            app.op = None;
            app.dc = None;
            app.trans = None;
            app.trans_selected_nodes.clear();
            app.trans_list_index = 0;
        }
        SimMsg::Op(op) => app.op = Some(op),
        SimMsg::Dc(dc) => app.dc = Some(dc),
        SimMsg::Transient(tr) => app.trans = Some(tr),
        _ => {}
    }
    app.ensure_visible_tab();
}

fn format_parse_error(error: &SpicyError, source_map: &SourceMap) -> String {
    let mut out = format!("Parse error: {error}");
    if let Some(span) = error.error_span() {
        let path = source_map.get_path(span.source_index);
        out.push_str(&format!("\n--> {}", path.display()));
        if let Some(snippet) = format_error_snippet(source_map.get_content(span.source_index), span)
        {
            out.push('\n');
            out.push_str(&snippet);
        }
    }
    out
}

pub fn worker_loop(netlist_path: PathBuf, rx: Receiver<SimCmd>, tx: Sender<SimMsg>) {
    while let Ok(cmd) = rx.recv() {
        match cmd {
            SimCmd::RunCurrentTab { config } => {
                let sim_config = config;
                let input = match std::fs::read_to_string(&netlist_path) {
                    Ok(input) => input,
                    Err(err) => {
                        let _ = tx.send(SimMsg::FatalError(format!(
                            "Failed to read netlist: {}",
                            err
                        )));
                        continue;
                    }
                };
                let mut parse_options = ParseOptions::new_with_source(&netlist_path, input);

                let lowered = match parse(&mut parse_options).and_then(|deck| lower(&deck)) {
                    Ok(lowered) => lowered,
                    Err(e) => {
                        let _ = tx.send(SimMsg::FatalError(format_parse_error(
                            &e,
                            &parse_options.source_map,
                        )));
                        continue;
                    }
                };

                let _ = tx.send(SimMsg::SimulationStarted(ResultNames::new(&lowered)));

                let (circuit, params) = (&lowered.circuit, &lowered.params);
                for analysis in &lowered.analyses {
                    match analysis {
                        Analysis::Op => {
                            match simulate_op(circuit, params, &sim_config) {
                                Ok(op) => {
                                    let _ = tx.send(SimMsg::Op(op));
                                }
                                Err(e) => {
                                    let _ = tx.send(SimMsg::FatalError(format!(
                                        "Simulation error: {}",
                                        e
                                    )));
                                }
                            }
                            continue;
                        }
                        Analysis::Dc(sweep) => {
                            let dc = simulate_dc(circuit, params, sweep, &sim_config);
                            let _ = tx.send(SimMsg::Dc(dc));
                            continue;
                        }
                        Analysis::Tran(tran) => {
                            match simulate_trans(circuit, params, tran, &sim_config) {
                                Ok(tr) => {
                                    let _ = tx.send(SimMsg::Transient(tr));
                                }
                                Err(e) => {
                                    let _ = tx.send(SimMsg::FatalError(format!(
                                        "Simulation error: {}",
                                        e
                                    )));
                                }
                            }
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
