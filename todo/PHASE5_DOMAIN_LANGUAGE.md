# Phase 5 Plan — Domain-Driven Design & Film Production Metaphor

**Created**: November 12, 2025  
**Status**: PLANNED  
**Prerequisites**: Phase 4 Complete  
**Estimated Effort**: 8-12 SP (2-3 sprints)

---

## Executive Summary

Phase 5 introduces **Domain-Driven Design (DDD)** principles to the codebase by adopting a **Film Production metaphor** for the rendering pipeline. This improves code comprehension, reduces onboarding time, and creates a shared vocabulary for the team.

**Core Philosophy**: Use familiar metaphors from analogous domains to reduce cognitive load when working with inherently complex systems.

**Key Principle**: The rendering pipeline is conceptually similar to film production—we position actors and props (instances), set up lighting and camera (render state), shoot takes (render passes), and composite layers (transparency). By using film production terminology, we make the code's intent immediately clear to new developers.

---

## The Film Production Metaphor

### Why Film Production?

1. **Direct Analogy**: Rendering *is* capturing a visual scene for display
2. **Rich Vocabulary**: Industry-standard terms with precise meanings
3. **Multiple Roles**: Director (orchestration), Cinematographer (camera/lighting), Editor (compositing)
4. **Widely Understood**: Most people have basic film knowledge
5. **Already Partial**: We use "scene" - extend the metaphor consistently

### Core Terminology Mapping

| Technical Term | Film Production Term | Rationale |
|----------------|---------------------|-----------|
| **Scene preparation** | **Blocking** / **Staging** | Positioning actors and props before shooting |
| **Render** | **Shoot** / **Capture** | Recording the visual frame |
| **Render passes** | **Takes** / **Shots** | Multiple captures (shadow, main, skybox) |
| **Transparent sorting** | **Compositing** | Layering transparent elements in post |
| **Instances** | **Actors** / **Props** | Renderable objects in the scene |
| **Camera setup** | **Framing** | Positioning and configuring the camera |
| **Material upload** | **Set Dressing** | Preparing visual properties |
| **Debug logging** | **Script Notes** | Recording production details |

---

## Proposed API Design

### Current API (Technical)

```rust
// moho_renderer/src/scene/preparation.rs
pub struct PreparedScene {
    pub cube_opaque: Vec<InstanceGpu>,
    pub sphere_opaque: Vec<InstanceGpu>,
    pub transparent_entries: Vec<(u32, InstanceGpu)>,
    pub materials_uploaded: bool,
}

pub struct ScenePreparation;
impl ScenePreparation {
    pub fn prepare(...) -> PreparedScene { }
}

// moho_renderer/src/scene.rs
impl Scene {
    pub fn render(...) {
        let prepared = ScenePreparation::prepare(...);
        renderer.render_mesh(...);
        self.render_transparent(...);
    }
}
```

### Proposed API (Film Production)

```rust
// moho_renderer/src/scene/blocking.rs (renamed from preparation.rs)
/// A scene ready for shooting - all actors positioned, lighting set, props dressed
pub struct BlockedScene {
    /// Opaque actors (rendered front-to-back for depth optimization)
    pub opaque_actors: OpaqueCast,
    /// Transparent actors (composited back-to-front)
    pub transparent_actors: Vec<(MeshHandle, InstanceGpu)>,
    /// Whether set dressing (materials) was uploaded this frame
    pub set_dressed: bool,
}

pub struct OpaqueCast {
    pub cubes: Vec<InstanceGpu>,
    pub spheres: Vec<InstanceGpu>,
}

/// The Director blocks the scene: positions actors, checks lighting, dresses the set
pub struct Director;

impl Director {
    /// Block the scene: prepare all actors, props, and set dressing for shooting
    /// 
    /// This is the "mise-en-scène" phase - everything must be in place before
    /// the cinematographer can capture the shot.
    pub fn block_scene(
        world: &mut World,
        material_table: &mut MaterialTable,
        buffer_manager: &mut BufferManager,
        instance_collector: &mut InstanceCollector,
        renderer: &mut dyn RendererBackend,
        mesh_handle: MeshHandle,
        cube_mesh_handle: MeshHandle,
    ) -> BlockedScene {
        // Step 1: Position actors (collect instances)
        instance_collector.collect_from_world(world, material_table, buffer_manager);
        
        // Step 2: Script notes (debug logging)
        Self::record_script_notes(instance_collector, buffer_manager);
        
        // Step 3: Set dressing (upload materials)
        let set_dressed = Self::dress_set(renderer, material_table);
        
        // Step 4: Separate cast by opacity
        let (opaque_actors, transparent_actors) =
            Self::organize_cast(instance_collector, mesh_handle, cube_mesh_handle);
            
        BlockedScene { opaque_actors, transparent_actors, set_dressed }
    }
    
    fn record_script_notes(...) { /* Debug logging */ }
    fn dress_set(...) -> bool { /* Material upload */ }
    fn organize_cast(...) -> (OpaqueCast, Vec<...>) { /* Transparency separation */ }
}

// moho_renderer/src/scene.rs
impl Scene {
    /// Shoot the scene: capture all render passes
    /// 
    /// The cinematographer executes the shot plan:
    /// 1. Shoot opaque actors (cubes, spheres)
    /// 2. Shoot voxel chunks (terrain)
    /// 3. Composite transparent layers in post-production
    pub fn shoot(
        &mut self,
        renderer: &mut dyn RendererBackend,
        world: &mut World,
        mesh_handle: MeshHandle,
        cube_mesh_handle: MeshHandle,
        camera: CameraSetup,
    ) {
        // Director blocks the scene
        let blocked = Director::block_scene(
            world,
            &mut self.material_table,
            &mut self.buffer_manager,
            &mut self.instance_collector,
            renderer,
            mesh_handle,
            cube_mesh_handle,
        );

        // Cinematographer captures opaque actors
        self.shoot_opaque_pass(renderer, &blocked.opaque_actors, camera);
        
        // Cinematographer captures voxel terrain
        self.shoot_terrain_pass(renderer, camera);
        
        // Editor composites transparent layers
        self.composite_transparency(renderer, blocked.transparent_actors, camera);
    }
    
    fn shoot_opaque_pass(...) { /* Render opaque geometry */ }
    fn shoot_terrain_pass(...) { /* Render voxel chunks */ }
    
    /// Composite transparent actors: sort by depth, render back-to-front
    /// 
    /// This is the post-production phase where the editor layers transparent
    /// elements to create proper alpha blending.
    fn composite_transparency(
        &self,
        renderer: &mut dyn RendererBackend,
        transparent_actors: Vec<(MeshHandle, InstanceGpu)>,
        camera: CameraSetup,
    ) {
        // Editor sorts by depth (back-to-front for alpha blending)
        let sorted = Self::depth_sort(transparent_actors, camera.position);
        
        // Group consecutive actors using same mesh (batch optimization)
        let batches = Self::batch_by_mesh(sorted);
        
        // Composite each batch
        for (i, batch) in batches.iter().enumerate() {
            let final_composite = i + 1 == batches.len();
            renderer.shoot_take(batch.mesh, &batch.actors, camera, final_composite);
        }
    }
}

// Type aliases for clarity
pub type MeshHandle = u32;
pub type CameraSetup = (glam::Mat4, glam::Mat4, glam::Vec3);
```

---

## Implementation Strategy

### Phase 5.1: Documentation & Planning (2 SP)

**Objective**: Document the metaphor and plan the migration

**Deliverables**:
1. ✅ `docs/DOMAIN_LANGUAGE.md` - Film production glossary
2. ✅ `docs/RENDERING_ARCHITECTURE.md` - Architecture with film terms
3. ✅ Migration plan with module-by-module breakdown
4. ✅ Risk assessment for renaming

**Activities**:
- Refine terminology (ensure film terms are precise)
- Map all rendering concepts to film production
- Identify breaking changes
- Plan gradual migration path

---

### Phase 5.2: Type Refinement (2 SP)

**Objective**: Introduce film-inspired types without breaking existing code

**Approach**: Add new types alongside existing ones

```rust
// Add type aliases first (non-breaking)
pub type BlockedScene = PreparedScene;
pub type OpaqueCast = (Vec<InstanceGpu>, Vec<InstanceGpu>);
pub type MeshHandle = u32;
pub type CameraSetup = (glam::Mat4, glam::Mat4, glam::Vec3);

// Export both names during transition
pub use preparation::{PreparedScene, BlockedScene};
```

**Benefits**:
- Zero breaking changes
- Gradual adoption possible
- Easy rollback if needed

---

### Phase 5.3: Module Renaming (2 SP)

**Objective**: Rename modules to reflect film production roles

**Changes**:
1. `scene/preparation.rs` → `scene/blocking.rs`
2. Add `scene/cinematography.rs` (if extracting camera/lighting logic)
3. Add `scene/compositing.rs` (if extracting transparency logic)

**Migration Path**:
```rust
// Phase 5.3a: Create new modules with new names
mod blocking;
pub use blocking::*;

// Phase 5.3b: Deprecate old module (keep for 1 release)
#[deprecated(since = "0.x.0", note = "Use `blocking` module instead")]
pub mod preparation {
    pub use super::blocking::*;
}

// Phase 5.3c: Remove deprecated module (breaking change)
```

---

### Phase 5.4: API Refinement (3 SP)

**Objective**: Rename primary APIs to use film terminology

**High-Priority Renames**:
```rust
// Core scene API
Scene::render() → Scene::shoot()
Scene::render_transparent() → Scene::composite_transparency()

// Preparation API
ScenePreparation::prepare() → Director::block_scene()
PreparedScene → BlockedScene

// Struct fields
PreparedScene.cube_opaque → BlockedScene.opaque_actors.cubes
PreparedScene.transparent_entries → BlockedScene.transparent_actors
PreparedScene.materials_uploaded → BlockedScene.set_dressed
```

**Medium-Priority Renames**:
```rust
// Internal methods
log_debug_info() → record_script_notes()
separate_by_transparency() → organize_cast()
upload_materials() → dress_set()

// Renderer backend
render_mesh() → shoot_take() // Consider carefully - widely used
```

**Low-Priority** (keep technical):
```rust
// These are fine as-is (standard graphics terms)
- shader, vertex, fragment
- buffer, uniform, binding
- pipeline, pass, attachment
```

---

### Phase 5.5: Documentation Update (1 SP)

**Objective**: Update all documentation with film terminology

**Files to Update**:
1. `README.md` - Explain the metaphor upfront
2. `docs/RENDERING_ARCHITECTURE.md` - Use film terms throughout
3. `moho_renderer/README.md` - Module-level docs
4. Inline doc comments - All public APIs
5. `CONTRIBUTING.md` - Vocabulary guide for contributors

**Documentation Template**:
```rust
/// Block the scene: position actors, dress the set, check lighting
///
/// # Film Production Metaphor
///
/// This function represents the **Director's blocking phase** in film production.
/// Before the cinematographer can shoot, the director must:
/// - Position actors (collect renderable instances)
/// - Dress the set (upload materials)
/// - Record script notes (debug logging)
/// - Organize the cast (separate by transparency)
///
/// # Technical Details
///
/// Internally, this:
/// - Queries ECS for `Cube`, `Sphere`, `VoxelChunk` components
/// - Uploads dirty materials to GPU via `renderer.set_materials()`
/// - Separates instances by transparency for correct rendering order
///
/// # Performance
///
/// This is called once per frame. Material uploads only occur when
/// `material_table.is_dirty()` returns true (typically after scene changes).
```

---

## Vocabulary Guide

### Film Production Glossary for Moho Renderer

| Film Term | Definition | Moho Usage |
|-----------|------------|------------|
| **Blocking** | Director positions actors/props before shooting | Scene preparation phase |
| **Shooting** | Cinematographer captures the visual frame | Render execution |
| **Take** | A single recorded attempt of a shot | Single render pass (shadow, main, etc.) |
| **Composite** | Editor layers multiple shots together | Transparency blending (back-to-front) |
| **Actor** | Person/object visible in frame | Renderable instance (Cube, Sphere) |
| **Prop** | Non-actor object in scene | Static geometry, terrain chunks |
| **Set Dressing** | Arranging visual elements of set | Uploading materials/textures |
| **Framing** | Camera position and composition | Camera setup (view/proj matrices) |
| **Lighting** | Illuminating the scene | Sun direction, ambient lighting |
| **Script Notes** | Recording production details | Debug logging |
| **Mise-en-scène** | "Everything in place" before shooting | Complete scene preparation |
| **Director** | Orchestrates overall production | Scene preparation coordinator |
| **Cinematographer** | Handles camera and lighting | Render execution |
| **Editor** | Post-production compositing | Transparency sorting/blending |

### When to Use Film Terms vs Technical Terms

**Use Film Terms** (High-Level Orchestration):
- ✅ Public APIs (`shoot_scene`, `block_scene`)
- ✅ Module names (`blocking.rs`, `cinematography.rs`)
- ✅ High-level structs (`BlockedScene`, `Director`)
- ✅ Documentation and comments

**Use Technical Terms** (Low-Level Implementation):
- ✅ GPU operations (`upload_buffer`, `bind_pipeline`)
- ✅ Graphics concepts (`shader`, `vertex`, `uniform`)
- ✅ WGPU/rendering APIs (`CommandEncoder`, `RenderPass`)
- ✅ Performance-critical code (profiler clarity)

**Hybrid Example**:
```rust
/// Shoot the opaque pass: render all non-transparent actors
///
/// Internally uses instanced rendering with GPU buffers.
pub fn shoot_opaque_pass(
    &self,
    renderer: &mut dyn RendererBackend,
    actors: &OpaqueCast,
    camera: CameraSetup,
) {
    // Film terminology in API, technical in implementation
    renderer.upload_instance_buffer(&actors.cubes);
    renderer.bind_pipeline(PipelineType::Opaque);
    renderer.draw_indexed(actors.cubes.len());
}
```

---

## Domain-Driven Design Principles

### Ubiquitous Language

**Definition**: The team uses the same vocabulary in code, docs, and conversation.

**Moho Application**:
- Code reviews: "The blocking phase is slow" vs "Preparation is slow"
- Issues: "Compositing drops transparent actors" vs "Render bug"
- Team chat: "Director should handle material uploads" vs "ScenePreparation needs refactor"

**Benefits**:
- Faster communication (no translation overhead)
- Clearer intent (film metaphor is intuitive)
- Better PRs (reviewers understand context)

---

### Bounded Contexts

**Definition**: Different subsystems can have different vocabularies.

**Moho Bounded Contexts**:

1. **Rendering Context** (Film Production Metaphor)
   - scene.rs, blocking.rs, cinematography.rs
   - Terms: shoot, block, composite, actor, take

2. **Input Context** (Musical Instrument Metaphor?)
   - key_mapping.rs, input_dispatcher.rs
   - Terms: key, binding, chord, press, release

3. **Audio Context** (Orchestra Metaphor?)
   - audio_system.rs, playback_state.rs
   - Terms: play, conduct, track, mix

4. **Physics Context** (Real World - Literal)
   - simulation.rs, voxel.rs
   - Terms: velocity, acceleration, collision (no metaphor needed)

**Key Insight**: Don't force one metaphor on the entire codebase. Each subsystem can have its own coherent vocabulary.

---

### Aggregates & Entities

**Definition**: Group related objects with clear boundaries and lifecycles.

**Moho Application**:

**Aggregate Root**: `Scene`
- Owns: `MaterialTable`, `BufferManager`, `InstanceCollector`
- Responsibility: Coordinate rendering lifecycle
- Film Role: **Production** (owns all production resources)

**Entity**: `BlockedScene`
- Lifecycle: Created per frame, consumed by render
- Film Role: **Prepared Set** (ready for shooting)

**Value Object**: `OpaqueCast`
- Immutable grouping of actors
- Film Role: **Cast List** (actors for this take)

---

## Benefits & Risks

### Benefits ✅

1. **Reduced Onboarding Time**: New developers grasp concepts faster
2. **Clearer Intent**: Code reads like the domain (film production)
3. **Shared Vocabulary**: Team uses same terms in code/docs/chat
4. **Better Mental Models**: Film metaphor aids comprehension
5. **Professional Codebase**: DDD is industry best practice
6. **Refactoring Guide**: Clear boundaries (Director, Cinematographer, Editor)

### Risks ⚠️

1. **Learning Curve**: Team must internalize the metaphor
2. **Metaphor Breakdown**: Not everything maps perfectly to film
3. **Over-Extension**: Risk of forcing metaphor where it doesn't fit
4. **Breaking Changes**: Renaming is disruptive (mitigate with gradual migration)
5. **Mixing Metaphors**: Must maintain consistency across codebase
6. **External Contributors**: May find metaphor confusing initially

### Mitigation Strategies

1. **Document Thoroughly**: `DOMAIN_LANGUAGE.md` explains all terms
2. **Gradual Migration**: Type aliases before breaking renames
3. **Know When to Stop**: Keep technical terms for GPU/low-level ops
4. **Team Buy-In**: Ensure everyone agrees on metaphor
5. **Glossary in Docs**: Quick reference for new contributors
6. **Hybrid Approach**: High-level metaphor, low-level technical

---

## Success Criteria

### Quantitative Goals

| Metric | Target | Measurement |
|--------|--------|-------------|
| **Onboarding Time** | -30% | Time for new dev to make first PR |
| **Code Review Clarity** | +40% | Self-reported understanding in PRs |
| **Documentation Completeness** | 100% | All public APIs have film-term docs |
| **Naming Consistency** | 90%+ | Film terms used in 90%+ of scene APIs |

### Qualitative Goals

1. ✅ New developers understand rendering pipeline from reading docs alone
2. ✅ Team naturally uses film terminology in discussions
3. ✅ Code reviews reference "blocking" and "compositing" without explanation
4. ✅ Architecture diagrams use film production workflow
5. ✅ Zero confusion about what high-level APIs do

---

## Testing Strategy

### API Compatibility Tests

```rust
#[test]
fn test_blocked_scene_is_prepared_scene() {
    // Ensure type aliases maintain compatibility
    let blocked: BlockedScene = create_test_scene();
    let prepared: PreparedScene = blocked; // Should compile
}

#[test]
fn test_shoot_is_render() {
    // Ensure renamed methods maintain behavior
    let mut scene = Scene::new();
    
    // Old API (deprecated but still works)
    #[allow(deprecated)]
    scene.render(...);
    
    // New API (preferred)
    scene.shoot(...);
    
    // Both should produce identical results
}
```

### Documentation Tests

```rust
/// Block the scene: position actors and dress the set
///
/// # Example
///
/// ```
/// use moho_renderer::{Scene, Director};
///
/// let blocked = Director::block_scene(
///     &mut world,
///     &mut material_table,
///     // ... other params
/// );
///
/// // Scene is now ready for shooting
/// assert!(!blocked.opaque_actors.cubes.is_empty());
/// ```
pub fn block_scene(...) -> BlockedScene { }
```

---

## Migration Timeline

### Sprint 1: Documentation & Planning (2 SP)
- ✅ Create `DOMAIN_LANGUAGE.md`
- ✅ Update `PHASE4_REFACTORING_PLAN.md`
- ✅ Plan detailed migration steps
- ✅ Get team buy-in on metaphor

### Sprint 2: Type Refinement (2 SP)
- Add `BlockedScene`, `OpaqueCast` type aliases
- Export both old and new names
- Update doc comments to mention film terms
- Zero breaking changes

### Sprint 3: Module Renaming (2 SP)
- Rename `preparation.rs` → `blocking.rs`
- Add deprecation warnings to old module
- Update internal imports
- Run full test suite

### Sprint 4: API Refinement (3 SP)
- Rename `Scene::render()` → `Scene::shoot()`
- Rename `ScenePreparation::prepare()` → `Director::block_scene()`
- Update all call sites
- Breaking changes (major version bump)

### Sprint 5: Documentation Polish (1 SP)
- Update README with film metaphor intro
- Rewrite architecture docs with film terms
- Add glossary to CONTRIBUTING.md
- Record video walkthrough using new terminology

**Total**: 10 SP across 5 sprints (~1.5 months)

---

## Integration with Phase 4

### Preparation Work During Phase 4

Even before Phase 5 starts, we can lay groundwork:

**During Track 14 (Event Loop Refactoring)**:
- Use "routing" terminology (film: "call sheet" routing actors)
- Consider "EventDirector" instead of "EventRouter"

**During Track 15 (Renderer Cleanup)**:
- Extract mesh rendering as "TakeCapture" or "ShotExecution"
- Think about film production roles when organizing code

**During Track 16 (Audio State Machine)**:
- Consider orchestra metaphor (conductor, instruments, performance)
- Align with overall DDD approach

**Documentation Updates**:
- Add section to Phase 4 doc about preparing for DDD
- Note opportunities to introduce domain language early

---

## Post-Phase 5 Vision

### Rendering Pipeline as Film Production

```
┌─────────────────────────────────────────────────────────┐
│                    FILM PRODUCTION                       │
│                   (Scene Rendering)                      │
├─────────────────────────────────────────────────────────┤
│                                                          │
│  1. PRE-PRODUCTION (Setup)                              │
│     └─ Scene::new() - Establish production facilities   │
│                                                          │
│  2. BLOCKING (Preparation)                              │
│     └─ Director::block_scene()                          │
│        ├─ Position actors (collect instances)           │
│        ├─ Dress set (upload materials)                  │
│        ├─ Check lighting (validate state)               │
│        └─ Record script notes (debug log)               │
│                                                          │
│  3. PRINCIPAL PHOTOGRAPHY (Rendering)                   │
│     └─ Scene::shoot()                                   │
│        ├─ Cinematographer::shoot_opaque_pass()          │
│        ├─ Cinematographer::shoot_terrain_pass()         │
│        └─ VFXSupervisor::shoot_transparent_pass()       │
│                                                          │
│  4. POST-PRODUCTION (Compositing)                       │
│     └─ Editor::composite_transparency()                 │
│        ├─ Sort by depth (back-to-front)                 │
│        ├─ Batch by mesh (optimization)                  │
│        └─ Composite layers (alpha blend)                │
│                                                          │
│  5. DISTRIBUTION (Display)                              │
│     └─ Present frame to viewer                          │
│                                                          │
└─────────────────────────────────────────────────────────┘
```

### Team Communication Examples

**Before** (Technical):
> "The scene preparation is taking 8ms, mostly in instance collection. Should we optimize the ECS query?"

**After** (Film Production):
> "Blocking is taking 8ms, mostly positioning actors. Should we optimize the cast collection?"

**Impact**: Same meaning, but film metaphor makes the problem space clearer. "Positioning actors" immediately suggests spatial optimization opportunities.

---

## Conclusion

Phase 5 transforms the Moho renderer from **technically correct** to **conceptually intuitive** by adopting Domain-Driven Design with a Film Production metaphor.

**Key Deliverables**:
1. Film production vocabulary for rendering pipeline
2. Gradual migration path with zero forced breaking changes
3. Comprehensive documentation with glossary
4. Improved team communication and onboarding

**Philosophy**: Great code doesn't just work—it **communicates intent**. By aligning our code with familiar mental models (film production), we make the inherently complex rendering pipeline **accessible** to new developers while maintaining technical precision.

**Next Steps**:
1. Complete Phase 4 (eliminate accidental complexity)
2. Gain team consensus on film metaphor
3. Begin Phase 5.1 (documentation and planning)
4. Execute gradual migration over 5 sprints

Let's make Moho not just powerful, but **beautiful to read**. 🎬
