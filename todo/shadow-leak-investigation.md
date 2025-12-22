# Shadow Leak / "Light-in-Crevice" Investigation

**Status:** Investigating
**Priority:** Critical (Blocking Phase 7)

## Problem Description
The user reports a persistent "light-in-crevice" issue (Peter Panning). Shadows appear detached from the base of objects, leaving a lit strip in the crevice where the object meets the ground. This persists despite previous bias tuning.

## Current State
- **Shadow Technique:** PCSS (Percentage Closer Soft Shadows) with 4 cascades (simulated via multi-light array).
- **Bias Strategy:** Slope-scale depth bias + Normal offset bias.
- **Geometry:** Hybrid (Smooth terrain + Blocky structures).

## Potential Root Causes

1.  **Face Culling in Shadow Pass:**
    *   If we cull *back* faces in the shadow pass (standard), we rely on bias to prevent acne.
    *   If we cull *front* faces (Peter Panning fix), we eliminate acne but cause leaking at the base if the mesh is thin or not watertight.
    *   *Action:* Check `cull_mode` in shadow pipeline.

2.  **Normal Offset Bias:**
    *   Pushing the shadow lookup position along the normal (`position + normal * offset`) moves it away from the surface.
    *   If the offset is too large, it moves the sample point out of the shadow volume of the object itself (for self-shadowing) or detaches it from the occluder.
    *   *Action:* Analyze `normal_offset` calculation in `fragment.wgsl`.

3.  **Slope-Scale Bias:**
    *   `depth - bias`. If bias is too large, the surface compares against a deeper shadow map value, thinking it's lit.
    *   *Action:* Analyze `slope_bias` calculation.

4.  **PCSS Blocker Search:**
    *   The blocker search samples a region. If the search radius is large and the blocker is small or close, it might miss the blocker or average it out.
    *   However, Peter Panning is usually a bias issue, not a softness issue.

5.  **Geometry Thickness:**
    *   If the terrain is a single layer of triangles (isosurface), normal offset might push the sample "under" the terrain, but for shadows, we care about the light's view.
    *   If the light sees the front face, and we push the sample towards the light (normal offset), we are safe from self-shadowing but might detach.
    *   Wait, normal offset usually pushes *out* along the normal.
    *   If the light is at an angle, pushing out might push it into the light.

## Investigation Plan

1.  [ ] **Audit Shadow Pipeline Configuration:** Check `cull_mode` and `depth_bias` settings in `moho_renderer`.
2.  [ ] **Audit Shader Bias Logic:** Review `shaders/fragment.wgsl` for bias math.
3.  [ ] **Hypothesis Testing:**
    *   Test 1: Disable Normal Offset entirely. Does acne return? Does leaking stop?
    *   Test 2: Check Face Culling.
    *   Test 3: Verify PCSS Blocker Search isn't ignoring close blockers.

## Findings

*   **Pipeline Configuration:**
        *   `cull_mode: Some(wgpu::Face::Back)` - Standard back-face culling.
        *   `depth_bias`: `constant: 2`, `slope_scale: 2.0`. This is a hardware depth bias applied *during shadow map generation*.
    *   **Shader Logic (`fragment.wgsl`):**
        *   `normal_offset`: `0.02 * (1.0 - ndotl)`
        *   `base_bias`: `0.0015`
        *   `slope_bias`: `0.005 * sqrt(...)`
        *   Total bias applied during *sampling*.

## Analysis

1.  **Double Bias:** We are applying bias in two places:
    *   **Hardware Bias:** `constant: 2`, `slope_scale: 2.0` in `ShadowSystem::new`. This pushes the shadow map depth values deeper.
    *   **Shader Bias:** `shadow_coords.z - bias` in `fragment.wgsl`. This pulls the comparison depth closer (or pushes the surface depth deeper relative to the light).
    *   *Hypothesis:* The combination of hardware bias AND shader bias AND normal offset is excessive, causing the "detached shadow" (leaking) effect.

2.  **Face Culling:**
    *   We are culling back faces (`wgpu::Face::Back`). This means we render front faces into the shadow map.
    *   This is the standard approach but requires bias to prevent self-shadowing (acne).
    *   If we switched to `wgpu::Face::Front` (culling front faces), we would render back faces. This naturally prevents acne on the lit side (since the shadow caster is the back face, which is deeper).
    *   *Risk of Front Face Culling:* If the mesh is not watertight or is thin (like a single quad for a wall), light might leak through the base because the back face is underground or non-existent.
    *   *Relevance:* The user mentions "crevices". If the terrain is a heightmap/isosurface, it might be single-sided. If we cull front faces, we might not render a shadow caster for the ground itself properly if the light is below the horizon (not an issue for sun) or if the geometry is thin.
    *   *However:* Front-face culling is a very robust way to fix acne without large biases. It might allow us to reduce the shader bias significantly, closing the gap.

3.  **Normal Offset:**
    *   `0.02` (2cm) is significant if the units are meters.
    *   If the crevice is small, 2cm might push the sample point out of the shadow of the adjacent block.

## Proposed Experiments

1.  **Disable Hardware Bias:** Set `constant: 0`, `slope_scale: 0.0` in `shadow.rs`. Rely only on shader bias.
2.  **Switch to Front-Face Culling:** Change `cull_mode` to `wgpu::Face::Front` in `shadow.rs`. Disable shader bias/normal offset.
    *   *Note:* This requires the geometry to be closed/thick. Voxel terrain is usually closed (chunks have back faces? No, usually just surface).
    *   Actually, voxel meshes usually cull internal faces. So a "block" is just 6 faces. If we cull front faces, we render the *inside* of the block.
    *   If the camera is outside, the light sees the front faces. If we cull them, the shadow map sees the *back* faces (inside the block).
    *   This works great for solid objects.
    *   For a "crevice" (two blocks meeting), the back face of block A is inside block A. The shadow starts at the back face.
    *   The front face of block A is at depth Z. The back face is at Z + thickness.
    *   The shadow map records Z + thickness.
    *   The fragment at Z compares itself to Z + thickness. Z < Z + thickness, so it is *lit*.
    *   Wait, if the shadow map records the *back* face (deeper), then the front face (shallower) will always pass the test `depth < shadow_depth` (lit).
    *   So self-shadowing is impossible (good for acne).
    *   But what about casting shadows?
    *   Block A casts shadow on Block B.
    *   Light ray hits Block A front face. Shadow map records Block A *back* face.
    *   Block B is behind Block A.
    *   If Block B is *between* Front A and Back A? Impossible if blocks are solid.
    *   If Block B is behind Back A, it is shadowed.
    *   **The Gap:** The region between Front A and Back A is "lit" according to the shadow map (because depth < Back A).
    *   But physically, that region is *inside* Block A. We don't see it.

## Experiment 1: Front-Face Culling & Zero Bias
**Action:**
- Modified `moho_renderer/src/shadow.rs`:
    - `cull_mode`: `wgpu::Face::Front`
    - `bias`: `constant: 0`, `slope_scale: 0.0`
- Modified `shaders/fragment.wgsl`:
    - `normal_offset`: `0.0`
    - `slope_bias`: `0.0`
    - `base_bias`: `0.00001` (minimal epsilon)

**Rationale:**
- Front-face culling renders the back faces of objects into the shadow map.
- This naturally prevents self-shadowing (acne) because the lit surface (front face) is always closer to the light than the shadow caster (back face).
- This allows us to remove the large biases that were causing the "Peter Panning" (detached shadow) artifact.
- The "gap" where light leaks is now restricted to the volume *inside* the object, which is invisible.

## Experiment 1 Results: FAILED
- **Observation:** The user reported that the entire map became dark/shadowed, except for edges.
- **Analysis:** This indicates that `surface_depth > shadow_depth` for almost all pixels. Since we were rendering back faces into the shadow map, this implies that the back faces were calculated as being *closer* to the light than the front faces. This could be due to:
    - Winding order mismatch (Front faces are actually CW?).
    - Geometry being "inside out" in some way.
    - Depth test confusion.
- **Conclusion:** Front-face culling is not a viable solution for this engine's current geometry pipeline. We must revert to Back-Face Culling.

## Experiment 2: Back-Face Culling + Single Bias Source
**Hypothesis:** The original "light-in-crevice" issue was caused by **Double Bias** (Hardware Bias + Shader Bias) pushing the shadow too far.
**Action:**
- Revert `cull_mode` to `wgpu::Face::Back` (Standard).
- **Keep Hardware Bias at 0**.
- Re-enable Shader Bias (`normal_offset`, `slope_bias`) but with tuned values.
- **Goal:** Eliminate the "Double Bias" factor. Control bias entirely in the shader where we can use `normal_offset` intelligently.

**Plan:**
1. `moho_renderer/src/shadow.rs`: Set `cull_mode: Back`, `bias: {0, 0, 0}`.
2. `shaders/fragment.wgsl`: Restore `normal_offset` (start small: 0.01) and `slope_bias`.

## Experiment 2 Results: FAILED
- **Observation:** The user reports that the "light-in-crevice" issue persists at the foot of hills.
- **Analysis:** Even with hardware bias disabled and reduced shader bias, the artifact remains. This suggests that the issue might not be solely about the *magnitude* of the bias, but perhaps the *direction* (normal offset on smoothed geometry) or the resolution/alignment of the shadow map itself.
- **User Feedback:** Suggests implementing debug visualizations (normals, bias values) to diagnose the root cause before further guessing.

## Phase 2: Visualization & Diagnosis Plan

**Objective:** Stop guessing bias values and *see* the data. We need to visualize the intermediate values used in the shadow calculation.

**Implemented Debug Modes:**
The shader now supports a `debug_mode` uniform, toggled via `r_debug_view <mode>`:

1.  **Mode 1: World Normals** (`r_debug_view 1`)
    *   **Visual:** `RGB = Normal * 0.5 + 0.5`.
    *   **Goal:** Check if terrain normals at the crevice are smoothed (curved up) or sharp.

2.  **Mode 2: Bias Heatmap** (`r_debug_view 2`)
    *   **Visual:** `Red = Slope Bias * 1000.0`.
    *   **Goal:** Identify if bias is exploding in the crevices.

3.  **Mode 3: Shadow Factor** (`r_debug_view 3`)
    *   **Visual:** `White = Lit`, `Black = Shadow`.
    *   **Goal:** See the raw shadow test result without lighting/texture noise.

4.  **Mode 4: Raw Light Level** (`r_debug_view 4`)
    *   **Visual:** `White = Light Level 15`, `Black = Light Level 0`.
    *   **Goal:** Verify voxel light propagation.

**Status:** Implemented. Ready for visual testing.

## Phase 2: Visualization Analysis
**Screenshots Received:** 5 images (Mode 0-4) at 7 AM (Low Sun Angle).

**Analysis:**
1.  **Mode 0 (Normal):** Confirms significant Peter Panning (detached shadows) at the base of hills.
2.  **Mode 1 (Normals):** Shows smooth normal transitions at the base of hills. The geometry is not sharp blocks but smoothed isosurface.
    *   *Implication:* `normal_offset` moves the sample point along this interpolated normal (45° up), potentially pushing it out of the shadow volume.
3.  **Mode 2 (Bias Heatmap):** **CRITICAL FINDING.** The heatmap is bright red almost everywhere.
    *   *Interpretation:* The calculated `slope_bias` is extremely high.
    *   *Cause:* At 7 AM, the sun angle is low. The dot product `ndotl` is small. Our formula divides by `max(ndotl, 0.1)`.
    *   *Math:* If `ndotl` ~ 0.1, `slope_bias` becomes very large (e.g., `0.01` or higher).
    *   *Result:* A bias of `0.01` (1% of depth range) is huge. It pulls the receiver depth significantly closer to the light, causing it to pass the depth test against the shadow map (appearing lit) even when it should be occluded.

**Conclusion:**
The "Light-in-Crevice" issue is primarily caused by **unbounded Slope-Scale Bias** at grazing angles (low sun). The bias becomes so large that it overrides the shadow depth difference.

## Experiment 3: Clamp Slope Bias
**Hypothesis:** Clamping the maximum slope bias will prevent the "explosion" at grazing angles while still preventing acne on steeper slopes.
**Action:**
- Modify `shaders/fragment.wgsl`:
    - Clamp `slope_bias` to a maximum value (e.g., `0.002`).
    - Possibly reduce the base multiplier.

**Plan:**
1.  Modify `fragment.wgsl` to clamp `slope_bias`.
2.  Retest with `r_debug_view 2` (should see less red) and `r_debug_view 0` (should see attached shadows).

## Experiment 3 Results: Inconclusive (Debug View Error)
- **Observation:** User reported "bias levels appear the same" (bright red heatmap) at all times of day.
- **Analysis:** The Debug Mode 2 code in `fragment.wgsl` was **re-calculating** the bias using the *old, unclamped formula*. It did not reflect the actual clamped bias used in the shadow calculation.
- **Correction:** Updated Debug Mode 2 to use the exact same clamped logic as `calculate_light_shadow`. Adjusted visualization scale (`* 500.0` instead of `* 1000.0`) so the clamped max (`0.0015`) appears as `0.75` (Bright Red) rather than saturated `1.5`.
- **Next Step:** Ask user to re-verify with the corrected debug view AND check Mode 0 (Normal Render) for the actual artifact.

## Experiment 4: Drastic Bias Reduction
- **Observation:** User reported "visuals are the same" (Peter Panning persists) after Experiment 3.
- **Analysis:**
    - Reviewed `moho_renderer/src/shadow.rs`.
    - Shadow Cascade Depth Range is approx **3000 meters** (Light Distance 2000m + Shadow Distance 1000m).
    - The previous bias of `0.0015` (0.15%) corresponds to `3000 * 0.0015 = 4.5 meters` of world-space bias.
    - **Root Cause:** The bias values were tuned for a much smaller depth range (e.g., 100m) and were orders of magnitude too large for the actual cascade setup.
- **Action:**
    - Reduced `base_bias` from `0.0005` to `0.000002` (~6mm).
    - Reduced `slope_bias` multiplier from `0.001` to `0.000005`.
    - Reduced `clamp` from `0.0015` to `0.00002` (~6cm).
    - Updated Debug Mode 2 scale to `40000.0` to visualize the new micro-bias.
- **Expectation:** Peter Panning should disappear. Shadow Acne *might* appear if the bias is too small for the depth precision.

## Experiment 5: Disable Normal Offset & Fix Instability
- **Observation:** User reported "visuals are the same" (gap persists) and "shadows flicker like flames" (instability).
- **Analysis:**
    - **Gap:** Even with minimal slope bias, the `normal_offset` was still pushing the sample point away from the surface. On curved terrain (crevices), the normal points *out* of the corner. Pushing along the normal moves the sample *away* from the hill, potentially exiting the shadow volume.
    - **Flickering:** The shadow map projection was not snapped to texels. As the camera moves, the shadow map grid slides continuously over the world, causing aliasing patterns (edges) to shimmer.
- **Action:**
    - **Gap Fix:** Disabled `normal_offset` completely in `fragment.wgsl`.
    - **Flickering Fix:** Implementing "Texel Snapping" in `shadow.rs`. This aligns the shadow camera to the shadow map texel grid, ensuring stable rasterization during movement.
- **Expectation:** The gap should close completely. The shadows should stop shimmering when the camera moves.

## Experiment 6: Fix Cascade Resolution & Snapping
- **Observation:** User reported "both issues still present" (gap + flickering).
- **Analysis:**
    - **Gap:** Found a critical bug in `shadow.rs`. `calculate_cascade_matrix` was ignoring the `near`/`far` split distances and using `SHADOW_DISTANCE` (1000m) for *all* cascades. This meant Cascade 0 (Near) had a radius of 1500m instead of ~400m, resulting in a texel size of **73cm**. This coarse resolution caused the "gap" (quantization error).
    - **Flickering:** The previous Texel Snapping implementation used `cascade_center` (camera position) as the reference point. Since the camera moves, the reference point moves, defeating the purpose of snapping.
- **Action:**
    - **Resolution Fix:** Updated `calculate_cascade_matrix` to use `far * 1.5` as the radius. For Cascade 0 (400m), this gives a radius of 600m and a texel size of **~29cm**. (Still somewhat large, but 2.5x better).
    -   **Snapping Fix:** Updated snapping logic to use `glam::Vec3::ZERO` (World Origin) as the stable reference point. This ensures the shadow grid remains locked to the world grid regardless of camera movement.
- **Expectation:** The gap should shrink significantly (from ~1m to ~30cm). Flickering should stop completely.

## Experiment 7: Fix Projection Matrix Z-Range (Clipping)
- **Observation:** User reports gap persists.
- **Analysis:**
    -   `shadow.rs` uses `glam::Mat4::orthographic_rh`. By default in `glam`, this produces a **-1 to 1** Z range (OpenGL convention).
    -   **WGPU** uses a **0 to 1** Z range (Vulkan/Metal/DX12 convention) for NDC.
    -   **Result:** WGPU clips all geometry with Z < 0 (the "near" half of the shadow volume).
    -   This means any object in the half of the shadow box closer to the light is **not rendered into the shadow map**.
    -   Additionally, `fragment.wgsl` transforms Z via `* 0.5 + 0.5`, which assumes a -1..1 input.
- **Action:**
    -   Implement `orthographic_rh_zo` (Zero-to-One) in `shadow.rs` to ensure the projection outputs 0..1 Z range.
    -   Update `fragment.wgsl` to **stop remapping Z** (only remap XY from -1..1 to 0..1).
- **Expectation:** The full shadow volume will be rendered. Objects previously clipped (causing gaps) will now cast shadows.

## Experiment 8: Fix Winding Order / Culling
- **Observation:** User reports "ground shadows missing".
- **Analysis:**
    -   The manual 0..1 projection matrix maps View Space -Z to NDC +Z. This is a reflection, which **flips the winding order** of triangles (CCW becomes CW).
    -   `shadow.rs` uses `front_face: Ccw` and `cull_mode: Back`.
    -   Since Front Faces (CCW) become CW after projection, and `cull_mode: Back` culls CW faces (assuming Back=CW when Front=CCW), the **Front Faces are being culled**.
    -   The shadow map is therefore rendering **Back Faces** (the inside of objects).
    -   Back Face Depth > Front Face Depth.
    -   Shadow Test: `ShadowDepth (Back) < ReceiverDepth (Front)`?
    -   `Back < Front` is FALSE.
    -   Result: The test fails, and the pixel is considered LIT.
- **Action:**
    -   Change `cull_mode` to `wgpu::Face::Front` in `shadow.rs`.
    -   This tells the pipeline to cull the faces defined as "Front" (CCW).
    -   Since our original Back Faces are now CCW (due to flip), they will be culled.
    -   Our original Front Faces (now CW) will be preserved.
- **Expectation:** Shadows should reappear on the ground.

## Experiment 9: Robust Matrix & Double-Sided Shadows
- **Observation:** User reports "geometry missing" (ambiguous, likely means shadows missing or artifacts).
- **Analysis:**
    -   Manual matrix construction might have been flawed or winding order assumptions were wrong.
    -   `cull_mode: Front` might have culled everything if winding wasn't flipped as expected.
- **Action:**
    -   Revert to `glam::Mat4::orthographic_rh` (Standard) to ensure valid projection.
    -   Apply a **Correction Matrix** to map -1..1 Z to 0..1 Z (WGPU standard).
    -   Set `cull_mode: None` (Disable culling). This renders BOTH front and back faces into the shadow map.
    -   **Benefit:** This guarantees that geometry is present in the shadow map regardless of winding order. It also helps with "crevice" leaks by ensuring the thickest possible blocker.
- **Expectation:** Shadows should definitely appear.

## Experiment 10: Revert Culling to Back
- **Observation:** User reports "geometry missing" with `cull_mode: None`.
- **Analysis:**
    -   It's possible that `cull_mode: None` caused unexpected artifacts or performance issues, or the user simply prefers the standard approach.
    -   With the **Correction Matrix** now correctly mapping Z to 0..1, the standard `cull_mode: Back` should work correctly (culling back faces, rendering front faces).
    -   This is the standard configuration for most engines (unless using the "render back faces" trick for acne, which we abandoned).
- **Action:**
    -   Revert `cull_mode` to `Some(wgpu::Face::Back)`.
    -   Keep the Correction Matrix and Standard Projection.
- **Expectation:** Shadows should appear correctly, without gaps (due to resolution fix) and without flickering (due to snapping fix).
