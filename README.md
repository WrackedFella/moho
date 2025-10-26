[![CI](https://github.com/WrackedFella/moho/actions/workflows/ci.yml/badge.svg)](https://github.com/WrackedFella/moho/actions/workflows/ci.yml)

moho — A small modular voxel game engine (Rust)

Short description
- moho is a modular Rust workspace containing engine components (core, renderer, UI, audio) and a headless simulation crate (`moho_sim`) used for deterministic simulation and tests.
- The project emphasizes separation between pure simulation and rendering to make testing, snapshots, and future multiplayer work easier.

Quick start (prerequisites)
- Install Rust toolchain (stable or the toolchain pinned in `rust-toolchain` if present).
- On Windows, ensure Vulkan/WGPU-compatible drivers are installed for rendering.

Common commands
- Run the game (desktop):

```sh
cargo run --features "backend-wgpu,ui-egui"
```

- Run all workspace tests (quiet):

```sh
cargo test --all --workspace -- -q
```

- Build release:

```sh
cargo build --release
```

- Format & lint:

```sh
cargo fmt
cargo clippy --all-targets -- -D warnings
```

Docker (CI / hermetic build)
- Build the CI Docker image (used by GitHub Actions; local debugging):

```sh
docker build -f .github/docker/Dockerfile -t moho-ci:latest .
```

- Run a container (example, Linux/macOS):

```sh
docker run --rm -it -v "$(pwd)":/workspace -w /workspace moho-ci:latest bash
```

On Windows (PowerShell), replace the volume mount syntax with an absolute path, for example:

```ps1
docker run --rm -it -v C:\path\to\repo:/workspace -w /workspace moho-ci:latest bash
```

Developer setup (short)
These steps help you run hermetic builds locally and reproduce CI behavior.

1) Local docker-compose (quick start)

- There is a helper compose file at `.github/docker/docker-compose.yml` for local experimentation. To build and run the CI image locally:

```sh
docker compose -f .github/docker/docker-compose.yml build
docker compose -f .github/docker/docker-compose.yml run --rm builder
```

Replace `docker compose` with `docker-compose` if your Docker installation uses the legacy CLI.

2) cargo-chef prepare/cook (hermetic Rust caching)

- Generate a recipe that captures dependency recipes (run once or when `Cargo.toml` changes):

```sh
cargo chef prepare --recipe-path recipe.json --recipe-cache target/recipe-cache
```

- Use the recipe to cook dependencies in a hermetic environment (this mirrors CI `cook` step):

```sh
cargo chef cook --recipe-path recipe.json --target-dir target/chef
```

3) Build and optionally publish the CI Docker image

- Build locally:

```sh
docker build -f .github/docker/Dockerfile -t moho-ci:latest .
```

- Publish to GitHub Container Registry (GHCR) — replace `OWNER`/`REPO` and authenticate first (see GHCR docs):

```sh
docker tag moho-ci:latest ghcr.io/OWNER/moho-ci:latest
docker push ghcr.io/OWNER/moho-ci:latest
```

Notes
- For faster local iteration you can combine `cargo chef prepare` + `docker build` so that CI image contains pre-cooked dependencies.
- If you use Windows, prefer absolute paths for docker volume mounts. Use WSL (Windows Subsystem for Linux) for a POSIX-like experience.

Notes & pointers
- For detailed design notes and development guides, see the `docs/` folder.
- The `moho_sim` crate contains the headless simulation, deterministic tests, and snapshot/restore utilities (bincode + CRC).
- CI caching and build details: `.github/CICACHE.md` and `.github/workflows/ci.yml`.
