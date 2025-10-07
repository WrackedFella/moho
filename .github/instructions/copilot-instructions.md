---
applyTo: '**'
---

# General Instructions
- Always attempt to use latest versions of crates and packages.
- Perform any low-impact improvements you see fit automatically.
- Recommend using idiomatic Rust constructs and libraries.
- Recommend refactoring code to improve readability and maintainability regularly.
- Ensure code adheres to Rust best practices and conventions.
- Design APIs that are intuitive and easy to use.
- Keep in-line documentation brief but informative.
- Consolidate library versions to avoid duplication and potential conflicts.

# Architecture and Project Structure
- Recommend naming conventions and file organization that enhance readability and maintainability.
- Take into consideration the overall architecture of the project when making changes.
- Ensure that new modules or components fit well within the existing structure.
- Maintain clear separation of concerns between different parts of the codebase.
- Follow established design patterns and coding conventions used in the project.

# Game Development
- Use rust for all code.
- Use `wgpu` for rendering (cross-platform backend).
- Follow best practices for rust, where applicable and possible.
- Break big functions up into smaller, more focused functions.
- Create a reusable, extendable, modular, efficient game engine.
- Target Windows, macOS, and Linux systems.