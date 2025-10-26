CI cache usage and keys

This file documents how the CI caching is configured and how to manually invalidate or inspect caches.

Overview
- We use a hybrid cargo-chef + sccache approach to speed up CI.
- The workflow builds a Docker image (Linux) and runs cargo-chef cook inside that image; macOS and Windows runners run `cargo chef cook` natively.
- Caches used:
  - Cargo registry & git: cached by `actions/cache` at `~/.cargo/registry` and `~/.cargo/git` (key includes `Cargo.lock` and `Cargo.toml` hashes).
  - cargo-chef build artifacts: cached under `target/deps` and `target/debug/deps` keyed by `recipe.json` hash per OS.
  - sccache: cached at `~/.cache/sccache` on Linux/macOS.

Cache keys
- Cargo registry cache key:
  `${{ runner.os }}-cargo-reg-${{ hashFiles('**/Cargo.lock') }}-${{ hashFiles('**/Cargo.toml') }}`
- sccache cache key (Linux/macOS):
  `${{ matrix.os }}-sccache-${{ hashFiles('**/Cargo.lock') }}`
- cargo-chef cache key (per OS):
  `${{ matrix.os }}-cargo-chef-${{ hashFiles('recipe.json') }}`

Notes & best practices
- Invalidation: change in `Cargo.lock` or `Cargo.toml` will change the cache keys and automatically cause a rebuild. If you need to force-invalidate a cache manually, go to the Actions -> Caches page in GitHub and delete the relevant cache entry.
- First run (cold cache): the first cook/build run will be full and slow; subsequent runs should be faster as caches warm.
- OS-specific recipes: We currently run cargo-chef cook on each OS (Linux/macOS/Windows). If you make OS-specific system dependency changes, regenerate the recipe as needed.
- Docker images: the workflow builds the CI Docker image on Linux. If you publish a prebuilt image (e.g., GHCR), you can reduce CI image build time by pulling the image instead.

Troubleshooting
- If you see strange build failures after a runner image update, try invalidating the cargo-chef cache for that OS and rerun to rebuild artifacts.
- If sccache appears to be reporting misses frequently, check `sccache --show-stats` output in CI logs.

Contact
- If you need a change to caching strategy (e.g., add `target` caching or publish CI images), open an issue or ping the team in the repo.