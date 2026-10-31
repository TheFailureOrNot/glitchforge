# Getting Started

## Prerequisites

- **Rust toolchain** (stable) – Install via [rustup](https://rustup.rs/)
- **Git** – For cloning the repository
- **Windows 10/11** – The project targets Windows platforms.

## Clone the Repository

```bash
git clone <repository-url>
cd glitchforge-rust
```

Replace `<repository-url>` with the actual URL of the project.

## Build the Project

```bash
cargo build --release
```

The compiled binary will be placed in `target\release\glitchforge.exe`.

## Run the Demo

```bash
cargo run --release
```

Or run the executable directly:

```bash
./target/release/glitchforge.exe
```

## Editing Effects

The effect system resides in `src/effects.rs`. To add a new effect:

1. Define a struct implementing the `Effect` trait.
2. Register the effect in the `EffectRegistry`.
3. Use the effect in your game logic.

## Editing the GUI

The UI utilities are in `src/gui.rs`. Create new UI components by extending the `Widget` trait and adding them to the `GuiRoot`.

For more detailed examples, see the `examples/` directory (if present).
