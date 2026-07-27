<div align="center">

# 🦎 Molt

**A local-first terminal UI and CLI for managing Python virtual environments and running code.**

[![Rust](https://img.shields.io/badge/Rust-1.70%2B-orange.svg?style=for-the-badge&logo=rust)](https://www.rust-lang.org/)
[![Platform](https://img.shields.io/badge/Platform-Linux-blue.svg?style=for-the-badge&logo=linux)](https://github.com/tomas-trantina/molt)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg?style=for-the-badge)](LICENSE)

<br />

<img width="100%" alt="Molt Terminal UI Preview" src="https://github.com/user-attachments/assets/ffe7fb95-a357-4601-9825-3f9e4d33406a" />

</div>

---

> 📌 **Status:** First complete implementation. Requires a Rust toolchain to build from source.

---

## ✨ Features

- ⚡ **Dual Backends, Unified Workflow**: Transparently uses [`uv`](https://github.com/astral-sh/uv) when available for ultra-fast operations, automatically falling back to standard `venv` + `pip`. Choose per project or globally.
- 📦 **One-Keypress Installations**: Press `i` in the TUI (or run `molt install`) to instantly install dependencies from `requirements.txt`.
- 🚀 **Script & Task Runner**: Execute scripts and saved tasks with active environment variables while streaming output into an interactive, scrollable console.
- 🛠️ **Package Management**: Inspect, add, and remove Python packages directly without leaving the UI.
- 🎯 **Command Palette & Help**: Instant action access via Command Palette (`a`) and a discoverable help overlay (`?`).
- ⚙️ **Fully Configurable**: Customize themes, colors, keybindings, backends, Python versions, and shells via `~/.config/molt/config.toml`.
- 📂 **Local-First & Portable**: Project registry and settings reside in standard XDG directories, while portable project settings live in `.molt.toml`.

---

## ⚡ Quick Start

```bash
# Register an existing project (or create a new one)
molt import ./my-project
molt create my-app --python 3.12

# Launch the interactive Terminal UI
molt

# Or use the CLI directly
molt env create                 # Create the default environment
molt install                    # Install requirements.txt into environment
molt add requests rich          # Install packages
molt run main.py                # Run a script in the environment
molt run test                   # Run a saved task named "test"
molt shell                      # Drop into an activated subshell
molt doctor                     # Check toolchain health
```

> 💡 **Global Flags:** `--project <name|path>`, `--env <name>`, `--backend <auto|uv|venv>`

---

## ⌨️ Keybindings

### 🖥️ Main Navigation

| Key | Action |
| :--- | :--- |
| `Arrows` | Navigate / focus panels |
| `Enter` | Select / focus / run |
| `a` | Command palette |
| `i` | Install requirements |
| `r` | Run selected task or script |
| `Tab` | Switch detail tab |
| `s` | Open activated shell |
| `n` | Import a project |
| `d` | Delete selected environment / package |
| `F5` | Reload project |
| `?` | Help overlay |
| `q` | Quit |

### 📜 Output Console

| Key | Action |
| :--- | :--- |
| `Arrows` / `PgUp` / `PgDn` | Scroll output |
| `Enter` | Re-enable auto-scroll |
| `x` | Stop running process |

---

## 📦 Installation

### ⚡ One-Line Quick Install (Linux / macOS)

```bash
curl -fsSL [https://raw.githubusercontent.com/tomas-trantina/molt/main/install.sh](https://raw.githubusercontent.com/tomas-trantina/molt/main/install.sh) | bash
```

### 🛠️ Local Installation

Clone the repository and run the installer script:

```bash
git clone [https://github.com/tomas-trantina/molt.git](https://github.com/tomas-trantina/molt.git)
cd molt
./install.sh
```

### 🔧 Manual Build from Source

Requires a working Rust toolchain:

```bash
# Build release binary
cargo build --release
# Compiled binary location: target/release/molt

# Run tests (pure logic, no network or Python required)
cargo test
```

---

## 📄 Project Manifest (`.molt.toml`)

Add portable, shareable per-project settings to your repository:

```toml
name = "my-app"
python = "3.12"
backend = "uv"          # optional: "auto" | "uv" | "venv"

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

---

## ⚙️ Configuration

Generate a default configuration or inspect paths:

```bash
molt config init  # Generates default ~/.config/molt/config.toml
molt config path  # Displays path to current active configuration
```

See [`config.example.toml`](./config.example.toml) for full details on customizing themes, keys, and backends.

---

## 🏗️ Architecture

Molt is built as a modular Rust crate, cleanly separating pure domain logic from system execution and rendering:

```text
src/
├── domain.rs      # Data model (projects, envs, tasks, packages) - pure logic, no I/O
├── config.rs      # Layered user configuration
├── theme.rs       # Color tokens + built-in themes (fully overridable)
├── keymap.rs      # Configurable key bindings
├── registry.rs    # Machine-local project registry
├── pyfinder.rs    # Python interpreter discovery
├── backend.rs     # Backend trait + uv and venv implementations
├── runner.rs      # Process execution (blocking, captured, streamed)
├── service.rs     # High-level operations shared by CLI and TUI
├── cli.rs         # Scriptable command-line interface
├── tui.rs         # Interactive terminal UI (powered by Ratatui)
└── lib.rs/main.rs # Core library & binary entry point
tests/
└── logic.rs       # Pure-logic integration tests
```

---

## 🗑️ Uninstallation

To remove Molt:

```bash
./uninstall.sh
# Or via curl:
# curl -fsSL [https://raw.githubusercontent.com/tomas-trantina/molt/main/uninstall.sh](https://raw.githubusercontent.com/tomas-trantina/molt/main/uninstall.sh) | bash
```

To also remove all user configuration and data files, pass the `--purge` flag:

```bash
./uninstall.sh --purge
```

---

## 📜 License

Distributed under the **MIT License**. See [`LICENSE`](./LICENSE) for more information.
