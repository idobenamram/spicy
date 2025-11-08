use bevy::prelude::*;
use serde_json::Value;

use super::trace::{Step, TraceFile};

#[derive(Resource)]
pub struct Player {
    pub paused: bool,
    pub speed: f32, // steps per second
    pub accum: f32,
}

impl Default for Player {
    fn default() -> Self {
        Self { paused: true, speed: 1.0, accum: 0.0 }
    }
}

#[derive(Resource)]
pub struct Grid {
    pub rows: usize,
    pub cols: usize,
    pub values: Vec<Option<f64>>,
    pub matching: Vec<isize>,
}

impl Default for Grid {
    fn default() -> Self {
        Self { rows: 5, cols: 5, values: vec![None; 25], matching: vec![-1; 5] }
    }
}

#[derive(Resource, Default)]
pub struct Trace {
    pub steps: Vec<Step>,
    pub idx: usize,
    pub code_lines: Vec<String>,
    pub active_code_line: u32,
    
    // Initial state
    initial: std::collections::HashMap<String, Value>,
    
    // Current state (reconstructed from initial + steps up to idx)
    pub column_permutations: Vec<isize>,
    pub cheap: Vec<usize>,
    pub visited: Vec<usize>,
    pub row_stack: Vec<usize>,
    pub column_stack: Vec<usize>,
    pub position_stack: Vec<usize>,
    pub head: i64,
    pub current_col: usize,
    pub current_row: usize,
    pub current_row_ptr: usize,
}

impl Trace {
    pub fn load_from(file: TraceFile, code_text: String) -> Self {
        let code_lines = code_text.lines().map(|s| s.to_string()).collect();
        
        // Extract initial state
        let column_permutations = file.initial
            .get("column_permutations")
            .and_then(|v| serde_json::from_value::<Vec<isize>>(v.clone()).ok())
            .unwrap_or_default();
        
        let cheap = file.initial
            .get("cheap")
            .and_then(|v| serde_json::from_value::<Vec<usize>>(v.clone()).ok())
            .unwrap_or_default();
        
        let visited = file.initial
            .get("visited")
            .and_then(|v| serde_json::from_value::<Vec<usize>>(v.clone()).ok())
            .unwrap_or_default();
        
        let row_stack = file.initial
            .get("row_stack")
            .and_then(|v| serde_json::from_value::<Vec<usize>>(v.clone()).ok())
            .unwrap_or_default();
        
        let column_stack = file.initial
            .get("column_stack")
            .and_then(|v| serde_json::from_value::<Vec<usize>>(v.clone()).ok())
            .unwrap_or_default();
        
        let position_stack = file.initial
            .get("position_stack")
            .and_then(|v| serde_json::from_value::<Vec<usize>>(v.clone()).ok())
            .unwrap_or_default();
        
        let mut trace = Self {
            steps: file.steps,
            idx: 0,
            code_lines,
            active_code_line: 1,
            initial: file.initial,
            column_permutations,
            cheap,
            visited,
            row_stack,
            column_stack,
            position_stack,
            head: 0,
            current_col: 0,
            current_row: 0,
            current_row_ptr: 0,
        };
        
        // Apply steps up to idx (which is 0, so this is just initialization)
        trace.apply_steps_to(0);
        
        trace
    }
    
    pub fn apply_steps_to(&mut self, target_idx: usize) {
        let max_idx = target_idx.min(self.steps.len());
        
        // Reconstruct from initial state
        self.reset_to_initial();
        
        // Apply each step up to target_idx
        for i in 0..max_idx {
            let step = self.steps[i].clone();
            match step {
                Step::Array { name, index, value, .. } => {
                    self.apply_array_update(&name, index, &value);
                }
                Step::Number { name, value, .. } => {
                    self.apply_number_update(&name, &value);
                }
                Step::Step { line } => {
                    self.active_code_line = line;
                }
            }
        }
        
        self.idx = max_idx;
    }
    
    fn reset_to_initial(&mut self) {
        self.column_permutations = self.initial
            .get("column_permutations")
            .and_then(|v| serde_json::from_value::<Vec<isize>>(v.clone()).ok())
            .unwrap_or_default();
        
        self.cheap = self.initial
            .get("cheap")
            .and_then(|v| serde_json::from_value::<Vec<usize>>(v.clone()).ok())
            .unwrap_or_default();
        
        self.visited = self.initial
            .get("visited")
            .and_then(|v| serde_json::from_value::<Vec<usize>>(v.clone()).ok())
            .unwrap_or_default();
        
        self.row_stack = self.initial
            .get("row_stack")
            .and_then(|v| serde_json::from_value::<Vec<usize>>(v.clone()).ok())
            .unwrap_or_default();
        
        self.column_stack = self.initial
            .get("column_stack")
            .and_then(|v| serde_json::from_value::<Vec<usize>>(v.clone()).ok())
            .unwrap_or_default();
        
        self.position_stack = self.initial
            .get("position_stack")
            .and_then(|v| serde_json::from_value::<Vec<usize>>(v.clone()).ok())
            .unwrap_or_default();
        
        self.head = 0;
        self.current_col = 0;
        self.current_row = 0;
        self.current_row_ptr = 0;
    }
    
    fn apply_array_update(&mut self, name: &str, index: usize, value: &Value) {
        match name {
            "column_permutations" => {
                if let Ok(val) = serde_json::from_value::<isize>(value.clone()) {
                    if index < self.column_permutations.len() {
                        self.column_permutations[index] = val;
                    }
                }
            }
            "cheap" => {
                if let Ok(val) = serde_json::from_value::<usize>(value.clone()) {
                    if index < self.cheap.len() {
                        self.cheap[index] = val;
                    }
                }
            }
            "visited" => {
                if let Ok(val) = serde_json::from_value::<usize>(value.clone()) {
                    if index < self.visited.len() {
                        self.visited[index] = val;
                    }
                }
            }
            "row_stack" => {
                if let Ok(val) = serde_json::from_value::<usize>(value.clone()) {
                    if index < self.row_stack.len() {
                        self.row_stack[index] = val;
                    }
                }
            }
            "column_stack" => {
                if let Ok(val) = serde_json::from_value::<usize>(value.clone()) {
                    if index < self.column_stack.len() {
                        self.column_stack[index] = val;
                    }
                }
            }
            "position_stack" => {
                if let Ok(val) = serde_json::from_value::<usize>(value.clone()) {
                    if index < self.position_stack.len() {
                        self.position_stack[index] = val;
                    }
                }
            }
            _ => {}
        }
    }
    
    fn apply_number_update(&mut self, name: &str, value: &Value) {
        match name {
            "head" => {
                if let Ok(val) = serde_json::from_value::<i64>(value.clone()) {
                    self.head = val;
                }
            }
            "col" => {
                if let Ok(val) = serde_json::from_value::<usize>(value.clone()) {
                    self.current_col = val;
                }
            }
            "row" => {
                if let Ok(val) = serde_json::from_value::<usize>(value.clone()) {
                    self.current_row = val;
                }
            }
            "row_ptr" => {
                if let Ok(val) = serde_json::from_value::<usize>(value.clone()) {
                    self.current_row_ptr = val;
                }
            }
            _ => {}
        }
    }
    
    pub fn next(&mut self) {
        if self.steps.is_empty() { return; }
        let new_idx = (self.idx + 1).min(self.steps.len());
        
        // Apply steps from current idx to new_idx
        for i in self.idx..new_idx {
            let step = self.steps[i].clone();
            match step {
                Step::Array { name, index, value, line } => {
                    self.active_code_line = line;
                    self.apply_array_update(&name, index, &value);
                }
                Step::Number { name, value, line } => {
                    self.active_code_line = line;
                    self.apply_number_update(&name, &value);
                }
                Step::Step { line } => {
                    self.active_code_line = line;
                }
            }
        }
        
        self.idx = new_idx;
    }
    
    pub fn prev(&mut self) {
        if self.idx == 0 { return; }
        let new_idx = self.idx - 1;
        
        // Reconstruct state by applying all steps up to new_idx
        self.apply_steps_to(new_idx);
    }
    

    pub fn extract_matrix_data(&self) -> (usize, usize, Vec<Option<f64>>) {
        let rows = self.initial
            .get("matrix_rows")
            .and_then(|v| serde_json::from_value::<usize>(v.clone()).ok())
            .unwrap_or(5);

        let cols = self.initial
            .get("matrix_cols")
            .and_then(|v| serde_json::from_value::<usize>(v.clone()).ok())
            .unwrap_or(5);

        let entries: Vec<(usize, usize, f64)> = self.initial
            .get("matrix_entries")
            .and_then(|v| serde_json::from_value::<Vec<(usize, usize, f64)>>(v.clone()).ok())
            .unwrap_or_default();

        let mut values = vec![None; rows * cols];
        for (col, row, val) in entries {
            if row < rows && col < cols {
                let idx = row * cols + col;
                if idx < values.len() {
                    values[idx] = Some(val);
                }
            }
        }

        (rows, cols, values)
    }
    
    pub fn get_matching(&self) -> Vec<isize> {
        self.column_permutations.clone()
    }
}

#[derive(Debug, Clone, Copy)]
pub enum HighlightKind {
    Cell,
    Row(usize),
    Col(usize),
}


