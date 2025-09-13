use spicy_simulate::{DcSweepResult, OperatingPointResult};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tab { Op, DC }

#[derive(Clone, Debug)]
pub struct Diagnostic { pub line: usize, pub msg: String }

#[derive(Debug)]
pub struct App {
    // Left pane
    pub path: String,
    pub netlist: Vec<String>,
    pub scroll: usize,
    pub diags: Vec<Diagnostic>,

    // Right pane
    pub tab: Tab,
    pub op: Option<OperatingPointResult>,
    pub dc: Option<DcSweepResult>,

    // Infra
    pub running: bool,
    pub focus_right: bool,
}

impl App {
    pub fn new(path: String, netlist_text: String) -> Self {
        Self {
            path,
            netlist: netlist_text.lines().map(|s| s.to_string()).collect(),
            scroll: 0,
            diags: Vec::new(),
            tab: Tab::Op,
            op: None,
            dc: None,
            running: false,
            focus_right: false,
        }
    }
}

pub fn prev_tab(tab: Tab) -> Tab {
    match tab {
        Tab::Op => Tab::DC,
        Tab::DC => Tab::Op,
        // Tab::Trans => Tab::Op,
        // Tab::Ac => Tab::Trans,
    }
}

pub fn next_tab(tab: Tab) -> Tab {
    match tab {
        Tab::Op => Tab::DC,
        Tab::DC => Tab::Op,
        // Tab::Trans => Tab::Ac,
        // Tab::Ac => Tab::Op,
    }
}

