# 0042 — Shipped assets resolve per file through folders the game names; placeholders are a folder

**Status:** Accepted (Justin, 2026-11-25); lands with ENG-F51-01, #952; amended 2026-11-27 (rule 6, ENG-F51-02 #996); amends [0012](0012-engine-crate-map-for-m2.md): the seam list gains asset roots and the install locator

## Context

ENG-F51 (#951, answering #948) makes a built game find the files it ships with, and lets a
developer, tester or artist name an asset folder at launch whose files override the committed
placeholders one file at a time. Today shipped files resolve three ways that do not survive a
shipped build: the build-time source path (`CARGO_MANIFEST_DIR`), the working directory, and a
path given at launch. Every later asset feature (R.5 import and streaming, ENG-M4 packaging,
mods supplying assets, the ENG-F5 repo split) builds on whichever contract lands first, so it
is fixed now, while only placeholders and a few sounds depend on it.

`moho_core::content::ContentRoots` already layers content folders (base, then mods) and
refuses a missing folder. Content and assets differ in how a layer wins: content merges every
layer by id (a mod adds or replaces one table in a file), an asset is a whole file and the
first layer that has it wins. Every loader the games use (`moho_level::load`, the audio sink,
glTF external buffers) takes a path today.

## Decision

1. **Assets resolve per file, top layer first.** An asset is named by a relative path inside a
   layer. Layers are ordered placeholders (bottom), then the launch-named folder, then mods
   (later, not built). Resolving a name returns the path in the highest layer that holds that
   file. A name that is absolute or holds a `..`, `.` or root component is refused, so no name
   can step outside the layers.
2. **Placeholders are a folder of files, not bytes embedded in the binary.** A shipped build
   carries its placeholder folder beside the executable; a dev run uses the committed folder.
   The loaders stay path-based, an override folder mirrors the placeholder layout, and a glTF
   takes its buffers and images from beside itself, whichever layer it came from.
3. **The engine type is given its folders; it never finds them.** `moho_core::assets::AssetRoots`
   takes every folder from its caller. It reads no environment variable, working directory or
   executable location. Each game binary locates its placeholder folder and reads the launch
   option; the option is the only way an override folder exists. Tests build their roots from
   the placeholders alone, so no machine-local folder or environment reaches them.
4. **Assets and content stay separate types sharing two rules.** `AssetRoots` is not folded
   into `ContentRoots`: their override semantics differ (first file wins vs merge by id). Both
   keep the same layer order and both refuse a named folder that does not exist, naming it.
5. **Every resolution reports its source, every miss lists where it looked.** Resolving logs
   the asset name, the layer that supplied it and that layer's folder. A name no layer holds
   is an error naming the asset and every folder searched, in search order.
6. **One engine locator decides where a build's shipped folders are, from paths it is given.**
   `moho_core::install::Install::locate(exe_dir, checkout, folders)` takes the executable's
   folder, the checkout root and the folder names a game ships. It reads no environment,
   working directory or executable location; each binary's `main` reads `current_exe()` and
   passes it in, so rule 3 still holds. If `exe_dir` holds any named folder it is the install
   and must hold all of them, or locating fails naming what is missing; a partial install
   never borrows from the checkout. If it holds none, the checkout is used and must hold all
   of them, or locating fails naming every folder searched in both places. The run logs which
   origin it used and its root.

   It lives in `moho_core`, beside `assets` and `content`, because both games call it
   (searchlight in ENG-F51-02, flagstone in ENG-F51-03 #997), it is platform-free path logic
   with no winit or renderer, and it is in the domain set so mutation testing covers it.
   `moho_app` would be the other shared home, but it carries winit and the renderer, which a
   headless path rule has no reason to pull in.

## Consequences

- A packaged build needs a step that lays the placeholder folder beside the executable;
  cargo does not provide one (ENG-F51-02 adds it). A lone executable without its folder does
  not run, and fails naming the folder it looked for.
- Exit 4 of ENG-F51 holds by construction: no code path turns an environment variable or a
  folder beside the test binaries into a layer. The executable's location is
  read only in each binary's `main` and handed to `Install::locate` (rule 6); neither
  `AssetRoots` nor `Install` nor test code reads it.
- A mod asset layer is one more `with_folder` above the launch folder; nothing in this
  decision changes for it.
- Packed archives, embedded fallbacks and streaming (R.5) supersede rule 2 if they arrive;
  rules 1, 3 and 5 are the contract they keep.
- Content definitions keep resolving through `ContentRoots`; the launch folder does not
  override them (#951 direction-setting decision).
- Install layout (the executable with each shipped folder beside it) is a one-way door:
  `just package <game>` produces it, testers receive it, and the repo split carries the
  `moho_core::install` API.

## Rejected

- **Locate inside each binary, no engine seam.** Two copies of the same partial-install rule,
  and #997's flagstone wiring and the repo split would lose the shared API. The card's tech
  spec would need rewriting.
