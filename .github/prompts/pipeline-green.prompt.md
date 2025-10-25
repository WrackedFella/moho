---
mode: agent
model: Grok Code Fast 1
---
Run the following commands together. If they produce errors or warnings, fix them then re-run all commands until they pass without issues

- cargo fmt --all -- --check
- cargo clippy --all-targets --all-features -- -D warnings
- cargo test --all --workspace -- -q
- cargo test -p moho_ui --features 'ui-egui ui-egui-test' -- -q
