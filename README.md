# bevy_golf

A mini golf game built with [Rust](https://www.rust-lang.org/) and the [Bevy](https://bevyengine.org/) game engine.

## 🕹️ Play Online

The game is automatically deployed to GitHub Pages on every push to `main`:

**[https://rlebell33.github.io/bevy_golf/](https://rlebell33.github.io/bevy_golf/)**

## 🚀 Running Locally

### Prerequisites

- [Rust](https://www.rust-lang.org/tools/install) (stable toolchain)

### Native (desktop)

```bash
cargo run
```

### Web (WASM) — local preview

Install [Trunk](https://trunkrs.dev/):

```bash
cargo install trunk
```

Add the WASM target:

```bash
rustup target add wasm32-unknown-unknown
```

Serve the game in your browser:

```bash
trunk serve
```

Then open <http://localhost:8080> in your browser.

## 🌐 GitHub Pages Deployment

The workflow at [`.github/workflows/deploy.yml`](.github/workflows/deploy.yml) runs automatically whenever code is pushed to `main`. It:

1. Checks out the repository.
2. Installs the Rust stable toolchain and the `wasm32-unknown-unknown` target.
3. Installs [Trunk](https://trunkrs.dev/) and [binaryen](https://github.com/WebAssembly/binaryen) (`wasm-opt`).
4. Runs `trunk build --release`, which compiles the game to WASM and produces an optimised bundle in the `dist/` directory.
5. Uploads the `dist/` directory as a GitHub Pages artifact and deploys it.

No manual steps are required — merging to `main` triggers a fresh deployment.

## 🛠️ Web Build Details

- **Build tool:** [Trunk](https://trunkrs.dev/) — reads `index.html` and `Trunk.toml`.
- **WASM size optimisation:** `wasm-opt -Oz` is applied via Trunk (`data-wasm-opt="z"` in `index.html`) and the Cargo release profile uses `opt-level = "z"` with LTO.
- **Base URL:** `Trunk.toml` sets `public_url = "./"` so all asset paths are relative, which is required for GitHub Pages sub-path hosting.