use spicy_parser::{error::SpicyError};
use spicy_simulate::{DcSweepResult, OperatingPointResult, TransientResult};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum Tab {
    Op,
    DC,
    Trans,
}

const TABS: [Tab; 3] = [Tab::Op, Tab::DC, Tab::Trans];

impl Tab {
    pub fn next(self) -> Tab {
        let idx = self as usize;
        let next_idx = (idx + 1) % TABS.len();
        TABS[next_idx]
    }
    pub fn prev(self) -> Tab {
        let idx = self as usize;
        let prev_idx = (idx - 1) % TABS.len();
        TABS[prev_idx]
    }
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

