---
applyTo: '**'
---

# General Instructions
- Always attempt to use latest versions of crates and packages when it is low-risk and non-breaking.
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
- Push back if user requests something that is not idiomatic or goes against best practices.
- Keep readme files to a minimum, remove old files as needed. Particularly plan files.

## Quality vs Speed Guidance
**Foundation/Engine Work (Current Phase):**
- Prioritize quality, future-proofing, and architectural soundness
- Focus on reusable, professional-grade systems that will scale
- Worth the extra effort to build solid foundations
- Examples: Event systems, physics integration, rendering pipeline, input routing

**Game-Specific Work (Future Phase):**
- Prioritize speed and pragmatic solutions
- Reuse is not required - optimize for specific use case
- Can use more targeted/hacky solutions when appropriate
- Examples: Specific gameplay mechanics, level-specific logic, UI screens for one game mode

When in doubt, ask: "Is this part of the reusable engine foundation or game-specific logic?"

## Agile & Planning Guidance
- Prefer Agile terminology: use Milestones for longer-term initiatives and Story Points for sizing work instead of hard time estimates.
- Avoid committing to hour-based or day-based estimates in instructions; use story points (e.g., 1, 2, 3, 5, 8) to convey relative effort and risk.
-- When describing implementation work in docs or PRIORITIES, label longer efforts as Milestones and break them down into user stories with acceptance criteria.
- Capture non-functional requirements and risks separately from story point estimates.
- When suggested timelines are provided, present them as "recommended sequencing" rather than strict schedules.

Examples:
- Instead of "Estimated Effort: 12-16 hours", use "Estimate: 5 SP (medium)".
- Instead of a fixed calendar plan, use "Milestone: Multiplayer - Next target: Input routing readiness (after HUD work)".

These changes help the team avoid false precision, improve planning flexibility, and better support iterative delivery.

# Architecture and Project Structure
- Recommend naming conventions and file organization that enhance readability and maintainability.
- Take into consideration the overall architecture of the project when making changes.
- Ensure that new modules or components fit well within the existing structure.
- Maintain clear separation of concerns between different parts of the codebase.
- Follow established design patterns and coding conventions used in the project.
- Consolidate library versions to avoid duplication and potential conflicts.
- When possible, lean towards product-ready architecture patterns and industry standards.
- Break large architectural changes into manageable phases that can be tested and validated.
- Prioritize professional-grade systems that will scale with the project's growth.

# Game Development
- Use rust for all code.
- Use `wgpu` for rendering (cross-platform backend).
- Follow best practices for rust, where applicable and possible.
- Break big functions up into smaller, more focused functions.
- Create a reusable, extendable, modular, efficient game engine.
- Target Windows, macOS, and Linux systems.