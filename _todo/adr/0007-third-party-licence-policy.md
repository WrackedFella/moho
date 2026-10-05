# 0007 — Third-party licences are compatible with closed-source commercial sale

**Status:** Accepted

## Context

The shipped games will be sold as closed-source binaries (confirmed
2026-10-02). `deny.toml` already rejects GPL/LGPL and allows a permissive set
plus MPL-2.0, but nothing records why, and nothing covers attribution or
non-crate assets.

Audit, 2026-10 (`cargo deny list`, all features):

- Permissive (MIT, Apache-2.0, BSD-2/3, ISC, Zlib, BSL-1.0, CC0, Unlicense,
  Unicode-3.0, 0BSD) cover all but the entries below.
- Dual-licensed crates with a copyleft alternative are taken under their
  permissive option: `ini`/`configparser` (MIT OR LGPL-3.0+), `self_cell`
  (Apache-2.0 OR GPL-2.0), `r-efi` (MIT OR Apache-2.0 OR LGPL-2.1+).
- MPL-2.0: `symphonia` codecs, through `rodio`. The copyleft is per file:
  using the crates unmodified places no obligation on our code.
- OFL-1.1 and Ubuntu Font Licence: egui's embedded default fonts
  (`epaint_default_fonts`). Bundling them in a sold program is allowed; the
  fonts may not be sold on their own.
- Non-crate assets: none in the repo. Shaders are our own code.
- Our own crates declare no licence and are not published.

## Decision

- Allowed: the current `deny.toml` allow-list. Adding a licence to it needs a
  new ADR or an amendment to this one.
- Copyleft beyond file-level (GPL, LGPL, AGPL) is never allowed, even
  through a dual licence that leaves the choice to us. MPL-2.0 is allowed only
  for unmodified upstream crates.
- Every distributed build ships a third-party notices file covering crate
  licences and asset licences. Generating it is part of the release
  checklist ([ENG-F5](../engine/ENG-F5-physical-repo-split/_feature.md) gate), not this gate.
- Assets (fonts, audio, textures, models) are added with their licence
  recorded next to them. An asset whose licence forbids commercial
  redistribution is never added.
- Workspace crates set `publish = false`.

## Consequences

- `just deny` remains the enforcement for crates. The asset rule is enforced
  in review until assets exist.
- If we modify an MPL-2.0 file (for example, by forking symphonia), we
  must publish that file's source.
- Swapping egui's default fonts for our own needs a licence check under
  this ADR.
