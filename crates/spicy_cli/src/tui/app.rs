use spicy_parser::error::SpicyError;
use spicy_simulate::{DcSweepResult, OperatingPointResult, SimulationConfig, TransientResult};

use crate::tui::nvim::NvimState;

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
        let prev_idx = (idx + TABS.len() - 1) % TABS.len();
        TABS[prev_idx]
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConfigField {
    Solver,
    Integrator,
    AbsTol,
    RelTol,
    MaxIters,
}

const CONFIG_FIELDS: [ConfigField; 5] = [
    ConfigField::Solver,
    ConfigField::Integrator,
    ConfigField::AbsTol,
    ConfigField::RelTol,
    ConfigField::MaxIters,
];

impl ConfigField {
    pub fn next(self) -> ConfigField {
        let idx = CONFIG_FIELDS
            .iter()
            .position(|f| *f == self)
            .unwrap_or(0);
        let next_idx = (idx + 1) % CONFIG_FIELDS.len();
        CONFIG_FIELDS[next_idx]
    }

    pub fn prev(self) -> ConfigField {
        let idx = CONFIG_FIELDS
            .iter()
            .position(|f| *f == self)
            .unwrap_or(0);
        let prev_idx = (idx + CONFIG_FIELDS.len() - 1) % CONFIG_FIELDS.len();
        CONFIG_FIELDS[prev_idx]
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
    pub nvim: Option<NvimState>,
    pub nvim_warning: Option<String>,

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
    pub show_help: bool,
    pub show_config: bool,
    pub config: SimulationConfig,
    pub config_field: ConfigField,
    pub config_edit: Option<String>,
    pub config_error: Option<String>,
}

impl App {
    pub fn new(path: String, netlist_text: String) -> Self {
        Self {
            path,
            raw_netlist: netlist_text.clone(),
            netlist: netlist_text.lines().map(|s| s.to_string()).collect(),
            scroll: 0,
            diags: Vec::new(),
            nvim: None,
            nvim_warning: None,
            tab: Tab::Op,
            op: None,
            dc: None,
            trans: None,
            trans_selected_nodes: Vec::new(),
            trans_list_index: 0,
            running: false,
            focus_right: false,
            show_help: false,
            show_config: false,
            config: SimulationConfig::default(),
            config_field: ConfigField::Solver,
            config_edit: None,
            config_error: None,
        }
    }
}
