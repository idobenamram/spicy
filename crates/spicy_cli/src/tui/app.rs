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


#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConfigField {
    Solver,
    Integrator,
    AbsTol,
    RelTol,
    MaxIters,
}

#[derive(Debug, Clone)]
pub struct ConfigEditState {
    pub buffer: String,
    pub error: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Modal {
    None,
    Help,
    Config,
}

pub(crate) const CONFIG_FIELDS: [ConfigField; 5] = [
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
    pub focus_right: bool,
    pub modal: Modal,
    pub config: SimulationConfig,
    pub config_field: ConfigField,
    pub config_edit: Option<ConfigEditState>,
}

impl App {
    pub fn new(path: String, netlist_text: String) -> Self {
        Self {
            path,
            raw_netlist: netlist_text,
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
            focus_right: false,
            modal: Modal::None,
            config: SimulationConfig::default(),
            config_field: ConfigField::Solver,
            config_edit: None,
        }
    }

    fn set_modal(&mut self, modal: Modal) {
        let leaving_config = self.modal == Modal::Config && modal != Modal::Config;
        let entering_config = self.modal != Modal::Config && modal == Modal::Config;
        self.modal = modal;
        if leaving_config || entering_config {
            self.clear_config_edit();
        }
    }

    pub fn netlist_line_count(&self) -> usize {
        self.raw_netlist.lines().count()
    }

    pub fn available_tabs(&self) -> Vec<Tab> {
        [
            (Tab::Op, self.op.is_some()),
            (Tab::DC, self.dc.is_some()),
            (Tab::Trans, self.trans.is_some()),
        ]
        .into_iter()
        .filter_map(|(tab, has_results)| has_results.then_some(tab))
        .collect()
    }

    pub fn selected_tab_index(&self, available_tabs: &[Tab]) -> usize {
        if available_tabs.is_empty() {
            return 0;
        }
        available_tabs
            .iter()
            .position(|tab| *tab == self.tab)
            .unwrap_or(0)
    }

    pub fn selected_tab(&self, available_tabs: &[Tab]) -> Option<Tab> {
        available_tabs
            .get(self.selected_tab_index(available_tabs))
            .copied()
    }

    pub fn ensure_visible_tab(&mut self) {
        let available_tabs = self.available_tabs();
        if available_tabs.is_empty() {
            return;
        }
        if !available_tabs.contains(&self.tab) {
            self.tab = available_tabs[0];
        }
    }

    pub fn left_pane_focused(&self) -> bool {
        !self.focus_right
    }

    pub fn right_pane_focused(&self) -> bool {
        self.focus_right
    }

    pub fn nvim_active(&self) -> bool {
        self.nvim.is_some() && !self.focus_right
    }

    pub fn left_pane_active(&self) -> bool {
        !self.focus_right && self.nvim.is_none()
    }

    pub fn is_help(&self) -> bool {
        self.modal == Modal::Help
    }

    pub fn is_config(&self) -> bool {
        self.modal == Modal::Config
    }

    pub fn toggle_help(&mut self) {
        let next = if self.modal == Modal::Help {
            Modal::None
        } else {
            Modal::Help
        };
        self.set_modal(next);
    }

    pub fn toggle_config(&mut self) {
        let next = if self.modal == Modal::Config {
            Modal::None
        } else {
            Modal::Config
        };
        self.set_modal(next);
    }

    pub fn close_help(&mut self) {
        if self.modal == Modal::Help {
            self.set_modal(Modal::None);
        }
    }

    pub fn close_config(&mut self) {
        if self.modal == Modal::Config {
            self.set_modal(Modal::None);
        }
    }

    pub fn clear_config_edit(&mut self) {
        self.config_edit = None;
    }
}
