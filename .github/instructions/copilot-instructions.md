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
- In-line comments should explain the "why" behind complex logic.
- Try to rely on self-documenting code rather than excessive comments.
- Use doc comments (`///`) for public APIs and complex functions.
- Readme files should be used to explain setup, architecture, and design decisions.
- De-prioritize suggestions or actions that might be called "future-proofing" and focus on getting features done now.
- Do suggest high-value future-proofing, and implement any low-effort ones as you see fit.
- Push back if user requests something that is not idiomatic or goes against best practices.

# Architecture and Project Structure
- Recommend naming conventions and file organization that enhance readability and maintainability.
- Take into consideration the overall architecture of the project when making changes.
- Ensure that new modules or components fit well within the existing structure.
- Maintain clear separation of concerns between different parts of the codebase.
- Follow established design patterns and coding conventions used in the project.
- Consolidate library versions to avoid duplication and potential conflicts.

# Game Development
- Use rust for all code.
- Use `wgpu` for rendering (cross-platform backend).
- Follow best practices for rust, where applicable and possible.
- Break big functions up into smaller, more focused functions.
- Create a reusable, extendable, modular, efficient game engine.
- Target Windows, macOS, and Linux systems.