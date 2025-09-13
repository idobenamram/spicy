# spicy_cli

Terminal CLI and TUI for Spicy.

## Modes

- non-TUI:

```bash
cargo run -p spicy_cli -- path/to/netlist.spicy
```

Parses the netlist and runs all commands found (`.OP`, `.DC`, `.AC`) using `spicy_simulate`.

- TUI mode:

```bash
cargo run -p spicy_cli -- --tui path/to/netlist.spicy
```

## Keybindings (TUI)

- `q`: quit
- `Tab`: toggle focus between left (netlist) and right (results)
- `j` / `k`: scroll netlist when left pane is focused
- `g`: jump to top (left pane focused)
- `G` (Shift+g): jump to bottom (left pane focused)
- `h` / `l`: previous/next tab
- `1` / `2`: select OP/DC tabs
- `r`: run simulation for the current tab