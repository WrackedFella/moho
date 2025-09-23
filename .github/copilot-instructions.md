# Copilot instructions — Moho (short)

---
applyTo: '**/*.rs'
---

Purpose: quick, repository-specific guidance for AI coding agents.

- Big picture:
	- Binary: `src/main.rs` builds a `legion::World` of `Sphere` components.
	- Engine core: `engine_core/` crate (use `engine_core::...`).
	- Renderer: `src/gpu.rs` (placeholder). Intended target: Vulkano instanced renderer.
	- Dataflow: `Sphere` -> `Sphere::to_instance()` -> `InstanceGpu` (POD) -> GPU upload/draw.

- Key files: `engine_core/src/actors.rs`, `src/main.rs`, `src/gpu.rs`, root `Cargo.toml`.

- Authoritative crate note: `engine_core/` is the canonical engine library. Ignore any leftover `core/` artifacts.

- Critical rules:
		1. Do NOT rename `engine_core` back to `core`.
		2. Keep CPU `InstanceGpu` layout and shader inputs in sync. Example: `engine_core/src/actors.rs` defines `InstanceGpu`; the test `engine_core/tests/instance_pod.rs` expects `std::mem::size_of::<InstanceGpu>() == 80`. If you change `InstanceGpu`, update shaders and this test.
	3. Preserve `run(instances, camera)` API while iterating the renderer.
	4. Make renderer/Vulkan edits incrementally and run `cargo build` after each change.

- Quick commands (PowerShell):
	- `cargo build`
	- `cargo run`
	- `cargo check`

- Safety checklist before PR:
	- `cargo build` passes
	- If `InstanceGpu` changed, update shaders and add a bytemuck size/assert test
	- Avoid large, single-step Vulkan rewrites; prefer small commits

	- Note: `src/gpu.rs` is intentionally a placeholder (no window). When implementing Vulkano, make small incremental changes and validate with `cargo build` between steps.

If you want, I can also move the long Rust guideline content to `.github/rust-conventions.md` and add CI/tests; tell me which to do next.
