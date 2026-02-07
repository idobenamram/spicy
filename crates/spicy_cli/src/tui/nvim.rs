use std::fmt;
use std::process::Command;
use std::sync::mpsc::{Receiver, TryRecvError};

use anyhow::{Context, Result};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use neovim_lib::{Neovim, NeovimApi, Session, UiAttachOptions, Value};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};

#[derive(Debug)]
pub enum NvimEvent {
    Saved(Option<String>),
    Help,
    Config,
    Quit,
}

pub struct NvimState {
    nvim: Neovim,
    events: Receiver<(String, Vec<Value>)>,
    grid: NvimGrid,
    cursor: Option<(u16, u16)>,
    grid_id: Option<i64>,
    last_size: (u16, u16),
    alive: bool,
}

impl fmt::Debug for NvimState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("NvimState")
            .field("grid", &self.grid)
            .field("cursor", &self.cursor)
            .field("grid_id", &self.grid_id)
            .field("last_size", &self.last_size)
            .field("alive", &self.alive)
            .finish()
    }
}

impl NvimState {
    pub fn spawn(path: &str, width: u16, height: u16) -> Result<Self> {
        let mut cmd = Command::new("nvim");
        cmd.arg("--embed").arg(path);
        let mut session = Session::new_child_cmd(&mut cmd).context("spawn nvim")?;
        let events = session.start_event_loop_channel();
        let mut nvim = Neovim::new(session);

        let width = width.max(1);
        let height = height.max(1);
        let mut opts = UiAttachOptions::new();
        opts.set_linegrid_external(true).set_rgb(false);
        nvim.ui_attach(width as i64, height as i64, &opts)
            .context("attach nvim ui")?;

        let lua = r#"
vim.g.mapleader = " "
vim.g.maplocalleader = " "
local function notify(name)
  vim.rpcnotify(0, name)
end
vim.keymap.set('n', '<leader>h', function() notify('spicy_help') end, { noremap = true, silent = true })
vim.keymap.set('n', '<leader>c', function() notify('spicy_config') end, { noremap = true, silent = true })
vim.keymap.set('n', '<leader>q', function() notify('spicy_quit') end, { noremap = true, silent = true })
vim.api.nvim_create_autocmd('BufWritePost', { buffer = 0, callback = function() vim.rpcnotify(0, 'spicy_save', vim.api.nvim_buf_get_name(0)) end })
"#;
        nvim.execute_lua(lua, Vec::new())
            .context("configure nvim bindings")?;

        Ok(Self {
            nvim,
            events,
            grid: NvimGrid::new(width, height),
            cursor: None,
            grid_id: None,
            last_size: (width, height),
            alive: true,
        })
    }

    pub fn is_alive(&self) -> bool {
        self.alive
    }

    pub fn resize_if_needed(&mut self, width: u16, height: u16) -> Result<()> {
        let width = width.max(1);
        let height = height.max(1);
        if (width, height) == self.last_size {
            return Ok(());
        }
        self.nvim
            .ui_try_resize(width as i64, height as i64)
            .context("resize nvim ui")?;
        self.last_size = (width, height);
        self.grid.resize(width, height);
        Ok(())
    }

    pub fn poll_events(&mut self) -> Vec<NvimEvent> {
        let mut out = Vec::new();
        loop {
            match self.events.try_recv() {
                Ok((name, args)) => match name.as_str() {
                    "redraw" => self.handle_redraw(args),
                    "spicy_save" => {
                        let path = args
                            .get(0)
                            .and_then(|val| val.as_str())
                            .map(|val| val.to_string());
                        out.push(NvimEvent::Saved(path));
                    }
                    "spicy_help" => out.push(NvimEvent::Help),
                    "spicy_config" => out.push(NvimEvent::Config),
                    "spicy_quit" => out.push(NvimEvent::Quit),
                    _ => {}
                },
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    self.alive = false;
                    break;
                }
            }
        }
        out
    }

    pub fn send_key(&mut self, key: KeyEvent) -> Result<()> {
        if !self.alive {
            return Ok(());
        }
        if let Some(input) = key_event_to_nvim_input(key) {
            self.nvim.input(&input).context("send nvim input")?;
        }
        Ok(())
    }

    pub fn render(&self, buffer: &mut Buffer, area: Rect, show_cursor: bool) {
        let cursor = if show_cursor { self.cursor } else { None };
        self.grid.render(buffer, area, cursor);
    }

    pub fn quit(&mut self) {
        let _ = self.nvim.quit_no_save();
    }

    fn handle_redraw(&mut self, args: Vec<Value>) {
        for event in args {
            let Value::Array(items) = event else {
                continue;
            };
            if items.is_empty() {
                continue;
            }
            let name = items[0].as_str().unwrap_or_default();
            for params in items.iter().skip(1) {
                let Value::Array(params) = params else {
                    continue;
                };
                match name {
                    "grid_resize" => self.handle_grid_resize(&params),
                    "grid_clear" => self.handle_grid_clear(&params),
                    "grid_line" => self.handle_grid_line(&params),
                    "grid_scroll" => self.handle_grid_scroll(&params),
                    "grid_cursor_goto" => self.handle_grid_cursor(&params),
                    "grid_destroy" => self.handle_grid_destroy(&params),
                    _ => {}
                }
            }
        }
    }

    fn is_target_grid(&mut self, grid: i64) -> bool {
        match self.grid_id {
            Some(id) => id == grid,
            None => {
                self.grid_id = Some(grid);
                true
            }
        }
    }

    fn handle_grid_resize(&mut self, params: &[Value]) {
        let Some(grid) = params.get(0).and_then(Value::as_i64) else {
            return;
        };
        if !self.is_target_grid(grid) {
            return;
        }
        let Some(width) = params.get(1).and_then(Value::as_i64) else {
            return;
        };
        let Some(height) = params.get(2).and_then(Value::as_i64) else {
            return;
        };
        let width = width.max(1) as u16;
        let height = height.max(1) as u16;
        self.grid.resize(width, height);
        self.last_size = (width, height);
    }

    fn handle_grid_clear(&mut self, params: &[Value]) {
        let Some(grid) = params.get(0).and_then(Value::as_i64) else {
            return;
        };
        if !self.is_target_grid(grid) {
            return;
        }
        self.grid.clear();
    }

    fn handle_grid_line(&mut self, params: &[Value]) {
        let Some(grid) = params.get(0).and_then(Value::as_i64) else {
            return;
        };
        if !self.is_target_grid(grid) {
            return;
        }
        let Some(row) = params.get(1).and_then(Value::as_i64) else {
            return;
        };
        let Some(col) = params.get(2).and_then(Value::as_i64) else {
            return;
        };
        let Some(Value::Array(cells)) = params.get(3) else {
            return;
        };
        self.grid.apply_grid_line(row as usize, col as usize, cells);
    }

    fn handle_grid_scroll(&mut self, params: &[Value]) {
        let Some(grid) = params.get(0).and_then(Value::as_i64) else {
            return;
        };
        if !self.is_target_grid(grid) {
            return;
        }
        let Some(top) = params.get(1).and_then(Value::as_i64) else {
            return;
        };
        let Some(bot) = params.get(2).and_then(Value::as_i64) else {
            return;
        };
        let Some(left) = params.get(3).and_then(Value::as_i64) else {
            return;
        };
        let Some(right) = params.get(4).and_then(Value::as_i64) else {
            return;
        };
        let Some(rows) = params.get(5).and_then(Value::as_i64) else {
            return;
        };
        let cols = params.get(6).and_then(Value::as_i64).unwrap_or(0);
        self.grid.scroll(
            top as usize,
            bot as usize,
            left as usize,
            right as usize,
            rows,
            cols,
        );
    }

    fn handle_grid_cursor(&mut self, params: &[Value]) {
        let Some(grid) = params.get(0).and_then(Value::as_i64) else {
            return;
        };
        if !self.is_target_grid(grid) {
            return;
        }
        let Some(row) = params.get(1).and_then(Value::as_i64) else {
            return;
        };
        let Some(col) = params.get(2).and_then(Value::as_i64) else {
            return;
        };
        self.cursor = Some((row as u16, col as u16));
    }

    fn handle_grid_destroy(&mut self, params: &[Value]) {
        let Some(grid) = params.get(0).and_then(Value::as_i64) else {
            return;
        };
        if self.grid_id == Some(grid) {
            self.grid.clear();
        }
    }
}

#[derive(Debug)]
struct NvimGrid {
    width: u16,
    height: u16,
    cells: Vec<char>,
}

impl NvimGrid {
    fn new(width: u16, height: u16) -> Self {
        let cells = vec![' '; width as usize * height as usize];
        Self {
            width,
            height,
            cells,
        }
    }

    fn resize(&mut self, width: u16, height: u16) {
        if self.width == width && self.height == height {
            return;
        }
        let mut next = vec![' '; width as usize * height as usize];
        let copy_w = self.width.min(width) as usize;
        let copy_h = self.height.min(height) as usize;
        for row in 0..copy_h {
            for col in 0..copy_w {
                let src = row * self.width as usize + col;
                let dst = row * width as usize + col;
                next[dst] = self.cells[src];
            }
        }
        self.width = width;
        self.height = height;
        self.cells = next;
    }

    fn clear(&mut self) {
        for cell in &mut self.cells {
            *cell = ' ';
        }
    }

    fn apply_grid_line(&mut self, row: usize, col: usize, cells: &[Value]) {
        if row >= self.height as usize || col >= self.width as usize {
            return;
        }
        let mut x = col;
        for cell in cells {
            let Value::Array(parts) = cell else {
                continue;
            };
            if parts.is_empty() {
                continue;
            }
            let text = value_to_string(&parts[0]).unwrap_or_else(|| " ".to_string());
            let repeat = parts
                .get(2)
                .and_then(Value::as_i64)
                .unwrap_or(1)
                .max(1) as usize;
            for _ in 0..repeat {
                for ch in text.chars() {
                    if x >= self.width as usize {
                        return;
                    }
                    self.set_cell(row, x, ch);
                    x += 1;
                }
                if text.is_empty() {
                    if x >= self.width as usize {
                        return;
                    }
                    self.set_cell(row, x, ' ');
                    x += 1;
                }
            }
        }
    }

    fn scroll(
        &mut self,
        top: usize,
        bot: usize,
        left: usize,
        right: usize,
        rows: i64,
        _cols: i64,
    ) {
        if top >= bot || left >= right {
            return;
        }
        let height = bot.saturating_sub(top);
        if height == 0 {
            return;
        }
        let shift = rows.abs() as usize;
        if shift == 0 || shift >= height {
            for row in top..bot {
                self.clear_row_segment(row, left, right);
            }
            return;
        }
        if rows > 0 {
            for row in top..(bot - shift) {
                self.copy_row_segment(row + shift, row, left, right);
            }
            for row in (bot - shift)..bot {
                self.clear_row_segment(row, left, right);
            }
        } else {
            for row in (top + shift..bot).rev() {
                self.copy_row_segment(row - shift, row, left, right);
            }
            for row in top..(top + shift) {
                self.clear_row_segment(row, left, right);
            }
        }
    }

    fn copy_row_segment(&mut self, src_row: usize, dst_row: usize, left: usize, right: usize) {
        if src_row >= self.height as usize || dst_row >= self.height as usize {
            return;
        }
        let right = right.min(self.width as usize);
        for col in left..right {
            let src = src_row * self.width as usize + col;
            let dst = dst_row * self.width as usize + col;
            self.cells[dst] = self.cells[src];
        }
    }

    fn clear_row_segment(&mut self, row: usize, left: usize, right: usize) {
        if row >= self.height as usize {
            return;
        }
        let right = right.min(self.width as usize);
        for col in left..right {
            let idx = row * self.width as usize + col;
            self.cells[idx] = ' ';
        }
    }

    fn set_cell(&mut self, row: usize, col: usize, ch: char) {
        if row >= self.height as usize || col >= self.width as usize {
            return;
        }
        let idx = row * self.width as usize + col;
        self.cells[idx] = ch;
    }

    fn render(&self, buffer: &mut Buffer, area: Rect, cursor: Option<(u16, u16)>) {
        if area.width == 0 || area.height == 0 {
            return;
        }
        let mut scratch = [0u8; 4];
        for y in 0..area.height {
            for x in 0..area.width {
                let grid_x = x as usize;
                let grid_y = y as usize;
                let ch = if grid_x < self.width as usize && grid_y < self.height as usize {
                    let idx = grid_y * self.width as usize + grid_x;
                    self.cells[idx]
                } else {
                    ' '
                };
                if let Some(cell) = buffer.cell_mut((area.x + x, area.y + y)) {
                    let symbol = ch.encode_utf8(&mut scratch);
                    cell.set_symbol(symbol);
                    let mut style = Style::default();
                    if let Some((cy, cx)) = cursor {
                        if grid_y == cy as usize && grid_x == cx as usize {
                            style = style.add_modifier(Modifier::REVERSED);
                        }
                    }
                    cell.set_style(style);
                }
            }
        }
    }
}

fn value_to_string(value: &Value) -> Option<String> {
    match value {
        Value::String(s) => s.as_str().map(|val| val.to_string()),
        Value::Binary(bytes) => Some(String::from_utf8_lossy(bytes).to_string()),
        _ => None,
    }
}

fn key_event_to_nvim_input(key: KeyEvent) -> Option<String> {
    let modifiers = key.modifiers;
    match key.code {
        KeyCode::Char(c) => {
            if modifiers.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) {
                let base = if c.is_ascii() {
                    c.to_ascii_lowercase().to_string()
                } else {
                    c.to_string()
                };
                return Some(format_modified_key(&base, modifiers));
            }
            Some(c.to_string())
        }
        KeyCode::Enter => Some(format_modified_key("CR", modifiers)),
        KeyCode::Backspace => Some(format_modified_key("BS", modifiers)),
        KeyCode::Tab => Some(format_modified_key("Tab", modifiers)),
        KeyCode::BackTab => Some("<S-Tab>".to_string()),
        KeyCode::Esc => Some(format_modified_key("Esc", modifiers)),
        KeyCode::Left => Some(format_modified_key("Left", modifiers)),
        KeyCode::Right => Some(format_modified_key("Right", modifiers)),
        KeyCode::Up => Some(format_modified_key("Up", modifiers)),
        KeyCode::Down => Some(format_modified_key("Down", modifiers)),
        KeyCode::Home => Some(format_modified_key("Home", modifiers)),
        KeyCode::End => Some(format_modified_key("End", modifiers)),
        KeyCode::PageUp => Some(format_modified_key("PageUp", modifiers)),
        KeyCode::PageDown => Some(format_modified_key("PageDown", modifiers)),
        KeyCode::Delete => Some(format_modified_key("Del", modifiers)),
        KeyCode::Insert => Some(format_modified_key("Insert", modifiers)),
        KeyCode::F(n) => Some(format_modified_key(&format!("F{n}"), modifiers)),
        _ => None,
    }
}

fn format_modified_key(base: &str, modifiers: KeyModifiers) -> String {
    let mut parts = Vec::new();
    if modifiers.contains(KeyModifiers::CONTROL) {
        parts.push("C");
    }
    if modifiers.contains(KeyModifiers::ALT) {
        parts.push("M");
    }
    if modifiers.contains(KeyModifiers::SHIFT) {
        parts.push("S");
    }
    if parts.is_empty() {
        format!("<{base}>")
    } else {
        format!("<{}-{base}>", parts.join("-"))
    }
}
