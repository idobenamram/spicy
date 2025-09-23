use spicy_parser::{error::SpicyError};
use spicy_simulate::{DcSweepResult, OperatingPointResult, TransientResult};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tab {
    Op,
    DC,
    Trans,
}

#[derive(Debug)]
pub struct App {
    // Left pane
    pub path: String,
    pub netlist: Vec<String>,
    pub raw_netlist: String,
    pub scroll: usize,
    pub diags: Vec<SpicyError>,

    // Right pane
    pub tab: Tab,
    pub op: Option<OperatingPointResult>,
    pub dc: Option<DcSweepResult>,
    pub trans: Option<TransientResult>,
    // Transient UI state
    pub trans_selected_nodes: Vec<usize>,
    pub trans_list_index: usize,

    // Infra
    pub running: bool,
    pub focus_right: bool,
}

impl App {
    pub fn new(path: String, netlist_text: String) -> Self {
        Self {
            path,
            raw_netlist: netlist_text.clone(),
            netlist: netlist_text.lines().map(|s| s.to_string()).collect(),
            scroll: 0,
            diags: Vec::new(),
            tab: Tab::Op,
            op: None,
            dc: None,
            trans: None,
            trans_selected_nodes: Vec::new(),
            trans_list_index: 0,
            running: false,
            focus_right: false,
        }
    }
}

pub fn prev_tab(tab: Tab) -> Tab {
    match tab {
        Tab::Op => Tab::DC,
        Tab::DC => Tab::Trans,
        Tab::Trans => Tab::Op,
        // Tab::Trans => Tab::Op,
        // Tab::Ac => Tab::Trans,
    }
}

pub fn next_tab(tab: Tab) -> Tab {
    match tab {
        Tab::Op => Tab::DC,
        Tab::DC => Tab::Trans,
        Tab::Trans => Tab::Op,
        // Tab::Trans => Tab::Ac,
        // Tab::Ac => Tab::Op,
    }
}
