# Molt

A local-first **terminal UI and CLI for managing Python virtual environments**
and running code inside them. Molt is built in Rust, targets Linux, and is
designed to be simple to drive, keyboard-first, and fully configurable.

> Status: first complete implementation. Requires a Rust toolchain to build.

## Highlights

- **Two backends, one workflow.** Uses [`uv`](https://github.com/astral-sh/uv)
  when it is available (fast) and transparently falls back to the standard
  library `venv` + `pip`. Choose per project or globally.
- **Install `requirements.txt` in one keypress.** Press `i` in the TUI (or run
  `molt install`) to install a requirements file into the selected environment.
- **Run scripts and saved tasks** with the environment activated, streaming the
  output into an in-app console you can scroll and stop.
- **Manage packages** - list, add, and remove packages without leaving the UI.
- **Command palette** (`a`) for every action, and a discoverable help overlay
  (`?`).
- **Everything is configurable** - themes, colours, key bindings, backend,
  Python version, confirmations, shells - via `~/.config/molt/config.toml`.
- **Local-first.** Project registry and settings live in standard XDG
  directories; portable per-project settings live in `.molt.toml`.

## Build

```bash
cargo build --release
# binary: target/release/molt
```

Run the tests (pure logic, no network or Python required):

```bash
cargo test
```

## Quick start

```bash
# Register an existing project (or create a new one)
molt import ./my-project
molt create my-app --python 3.12

# Launch the interactive UI
molt

# ...or use the CLI directly
molt env create                 # create the default environment
molt install                    # install requirements.txt into it
molt add requests rich          # install packages
molt run main.py                # run a script in the environment
molt run test                   # run a saved task named "test"
molt shell                      # drop into an activated subshell
molt doctor                     # check the toolchain
```

Global flags: `--project <name|path>`, `--env <name>`, `--backend <auto|uv|venv>`.

## Keyboard (defaults)

| Key | Action |
|-----|--------|
| arrows | navigate / focus |
| `enter` | select / focus / run |
| `a` | command palette |
| `i` | install requirements |
| `r` | run selected task or script |
| `tab` | switch detail tab |
| `s` | open activated shell |
| `n` | import a project |
| `d` | delete selected env / package |
| `f5` | reload project |
| `?` | help |
| `q` | quit |

In the output console: arrows / page keys scroll, `enter` re-enables
auto-scroll, `x` stops the running process.

## Project manifest (`.molt.toml`)

Portable, shareable per-project settings:

```toml
name = "my-app"
python = "3.12"
backend = "uv"          # optional override

[[environments]]
name = "default"
path = ".venv"
requirements = "requirements.txt"

[[tasks]]
name = "test"
kind = "module"          # script | module | shell | exec
command = "pytest"
args = ["-q"]

[[tasks]]
name = "serve"
kind = "script"
command = "main.py"
env_file = ".env"
```

## Configuration

See [`config.example.toml`](./config.example.toml) for every setting, or run
`molt config init` to write a default file and `molt config path` to locate it.

## Architecture

Modular Rust crate, cleanly separating concerns so new backends, themes,
languages, or screens can be added without rewriting the core:

```
src/
  domain.rs      data model (projects, envs, tasks, packages) - no I/O
  config.rs      layered user configuration
  theme.rs       colour tokens + built-in themes (fully overridable)
  keymap.rs      configurable key bindings
  registry.rs    machine-local project registry
  pyfinder.rs    Python interpreter discovery
  backend.rs     Backend trait + uv and venv implementations
  runner.rs      process execution (blocking, captured, streamed)
  service.rs     high-level operations shared by CLI and TUI
  cli.rs         scriptable command-line interface
  tui.rs         interactive terminal UI (ratatui)
  lib.rs/main.rs library + thin binary
tests/
  logic.rs       pure-logic integration tests
```

## License

MIT - see [LICENSE](./LICENSE).
