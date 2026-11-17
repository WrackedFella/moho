# 0036 — A crate may test against the GPU stack without depending on it

**Status:** Accepted on merge of #843; amends [0013](0013-ui-reaches-the-renderer-as-paint-data.md)

## Context

[0013](0013-ui-reaches-the-renderer-as-paint-data.md) keeps GPU code in `moho_renderer` so that
a wgpu upgrade and an egui upgrade each touch one crate. `scripts/layering.txt`'s `gpu` row
enforces it in `just check`: no other crate reaches `wgpu`, `egui-wgpu` or `naga` itself, and
dev-dependencies count.

[ENG-F49](https://github.com/WrackedFella/moho/issues/836) has tests drive `moho_ui_shell`
headless and, in its spike ([#838](https://github.com/WrackedFella/moho/issues/838)), render
one shell frame offscreen. The only headless egui harness, `egui_kittest`, renders through
`egui-wgpu`, so the shell's examples and tests need `wgpu`, `egui-wgpu` and
`egui_kittest[wgpu]` as dev-dependencies. Cargo builds dev-dependencies only for tests,
examples and benches, never into a crate's library or a game binary, so 0013's runtime rule is
untouched. Putting the shell in the `gpu` row would pass the gate but would also stop it
catching a runtime GPU dependency in the shell, which is what 0013 forbids.

## Decision

1. 0013's rule covers what a crate's library and binaries depend on (normal and build edges).
   A crate listed in `layering.txt`'s `gpu-dev` row may reach `wgpu`, `egui-wgpu` and `naga`
   through dev-dependencies only; tests, examples and benches may use them.
2. `just check` enforces both rows: a crate in `gpu` may reach them anywhere; a crate in
   `gpu-dev` may not reach them over normal or build edges (`gpu-at-runtime`); any other crate
   may not reach them at all (`gpu-outside-renderer`, unchanged).
3. `moho_ui_shell` is the first `gpu-dev` crate. Its GPU dev-dependencies follow egui, never
   the renderer: `egui-wgpu` and `egui_kittest` take egui's version, and its `wgpu` is the one
   that `egui-wgpu` uses. When the workspace's `wgpu` moves past it, the shell pins that older
   `wgpu` as its own dev-dependency instead of `{ workspace = true }`.
4. Another crate joins `gpu-dev` under these rules without a new ADR; the card that needs it
   lists `scripts/layering.txt` in its footprint and cites this ADR. A crate that needs the GPU
   stack at runtime still needs an ADR that supersedes 0013.

## Consequences

- A wgpu upgrade still changes only `moho_renderer`'s code and never waits on an egui release;
  a `gpu-dev` crate's tests may need the one-line pin in rule 3. An egui upgrade touches
  `moho_ui_shell` (now including its test harness pins) and `flagstone_ui`, as under
  [0017](0017-ui-shell-and-debug-values-are-engine-seams.md).
- `scripts/check-layering.sh` makes one more `cargo tree` call per crate (own reach over normal
  and build edges); the layering step takes about 4 s longer.
- The shell's test build adds `egui-wgpu` and `egui_kittest` on all three OSes; `wgpu` is
  already built for the renderer.
- [0005](0005-crate-lines-and-dependency-direction.md)'s lines and rules are unchanged; the
  `gpu` and `gpu-dev` rows enforce 0013 and this ADR, not 0005's table.
