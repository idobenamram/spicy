# Spicy Cli

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
- `h` / `?`: toggle help
- `c`: toggle config
- `Esc`: close help/config
- `Tab`: toggle focus between left (netlist) and right (results)
- `j` / `k`: scroll netlist when left pane is focused
- `g`: jump to top (left pane focused)
- `G` (Shift+g): jump to bottom (left pane focused)
- `Left` / `Right`: previous/next tab
- `1` / `2` / `3`: select OP/DC/Tran tabs
- `r`: run simulation for the current tab
- `Up` / `Down`: move selection in transient node list (right pane)
- `Enter`: toggle node in transient node list (right pane)

Config overlay:
- `Up` / `Down`: select field
- `Left` / `Right`: toggle solver/integrator
- `Enter`: edit numeric values
- `Backspace`: delete in edit mode
- `Esc`: cancel edit or close config