# ENG-F19 — Games load definitions and content from layered roots


## Summary

Items, weapons and similar are data, and data-only mods override them by id. Scripted mods are wanted later (own ADR), so behaviours are referenced by id. Consumer: FPS GDD v0.2 §10 (pending on `dev`); strategy is a likely second consumer.

## Exit criteria

- Typed definitions load from files (format per ENG-F13).
- Content roots load in order; a later root overrides an entry by stable string id (test).
- A load error names the file and the id.
- Saves record the active root list.
- Behaviours are referenced by id, so a future scripting ADR can add new ones without schema changes.

## Scope

- Out: scripting (future ADR); mod manager UI.

## Items

| Item |
|---|
