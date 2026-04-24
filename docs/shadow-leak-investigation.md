## Experiment 11: Fix Z-Range Remapping in Multi-Light Matrix
**Date:** 2024-05-24
**Hypothesis:** The "Missing Shadows" on the ground are caused by WGPU clipping the shadow map geometry. `glam::Mat4::orthographic_rh` produces Z in range `[-1, 1]`, but WGPU expects `[0, 1]`. The `calculate_light_matrix` function defined the `correction_matrix` (Z-remap) but **failed to apply it** in the return statement.
**Action:**
- Updated `moho_renderer/src/shadow.rs`:
  - In `calculate_light_matrix`, changed return value from `light_proj * correction * light_view` to `correction_matrix * light_proj * correction * light_view`.
**Expected Result:**
- The shadow map will now contain the full depth range of the scene.
- Shadows should reappear on the ground.
- This aligns the "Multi-Light" path with the "Cascade" path (which already had the fix).

## Experiment 12: Fix Light Position (Underground Sun)
**Date:** 2024-05-24
**Hypothesis:** The "Shadows render towards the light" and "Missing shadows at high angles" issues are caused by the light camera being positioned **underground**.
- `game_clock.rs` returns `sun_direction` pointing **TO** the sun (e.g., Up).
- `shadow.rs` calculated `light_pos = center - light_dir * dist`.
- This placed the light at `center - Up`, i.e., underground.
- An underground camera looking up sees the bottom of the terrain.
- Back-face culling removes the top faces (which are back-faces to the underground camera).
- The shadow map captures the bottom of the terrain.
- This inverts the shadow projection logic.
**Action:**
- Updated `moho_renderer/src/shadow.rs`:
  - Changed `light_pos` calculation to `center + light_dir * dist` in both `calculate_cascade_matrix` and `calculate_light_matrix`.
**Expected Result:**
- The light camera will be positioned in the sky, looking down.
- Shadows should project away from the light.
- Shadows should be visible on the ground.

## Experiment 13: Fix Shadow Banding (Acne)
**Date:** 2024-05-24
**Hypothesis:** The "Shadow Banding" (acne) visible on the terrain is caused by insufficient shadow bias.
- In Experiment 4, we drastically reduced the bias (`0.000002`) thinking the scene scale was small.
- We now know the scene scale is large (~3000m depth range).
- The small bias is insufficient to overcome the depth precision limits and surface self-shadowing, causing stripes.
**Action:**
- Updated `shaders/fragment.wgsl`:
  - Increased `base_bias` from `0.000002` to `0.00005` (~15cm).
  - Increased `slope_bias` factor from `0.000005` to `0.0001`.
  - Increased `slope_bias` clamp from `0.00002` to `0.0005`.
**Expected Result:**
- The dark bands on the terrain should disappear.
- Shadows should remain attached to objects (no Peter Panning) due to the slope-scale adjustment.

## Experiment 14: Enable Normal Offset Bias
**Date:** 2024-05-24
**Hypothesis:** The persistent "Shadow Banding" (acne) is caused by the large texel size (~0.73m) relative to the terrain slope.
- Even with large depth bias, grazing angles cause the depth to change by meters across a single texel.
- Depth bias alone cannot fix this without causing massive Peter Panning.
- **Normal Offset Bias** moves the sample point along the surface normal, effectively sampling a "safer" texel that is definitely in front of the surface.
**Action:**
- Updated `shaders/fragment.wgsl`:
  - Re-enabled `normal_offset`.
  - Set `normal_offset = 0.2 * (1.0 - ndotl)`.

## Experiment 15: Switch to Front Face Culling
**Date:** 2024-05-24
**Hypothesis:** The persistent "Shadow Banding" (acne) is due to self-shadowing precision errors when rendering front faces into the shadow map.
- Even with large bias and normal offset, the grazing angles on the terrain cause the depth comparison to fail intermittently.
- **Front Face Culling** (rendering back faces into the shadow map) is a robust technique for closed geometry.
- It pushes the shadow depth to the *far side* of the object.
- This naturally biases the shadow map away from the lit surface by the thickness of the object.
- Since we fixed the "Underground Sun" issue (Exp 12) and the "Z-Range" issue (Exp 11), Front Face Culling should now work correctly without causing "missing shadows" (unless the mesh is single-sided).
**Action:**
- Updated `moho_renderer/src/shadow.rs`:
  - Changed `cull_mode` from `Some(wgpu::Face::Back)` to `Some(wgpu::Face::Front)`.
**Expected Result:**
- Shadow acne (banding) should be completely eliminated.
- Shadows should remain correct for closed geometry (terrain chunks).
- No Peter Panning should be visible because the bias is effectively "object thickness".

## Experiment 16: Reduce Bias for Front Face Culling
**Date:** 2024-05-24
**Hypothesis:** The "Detached Shadows" (Peter Panning) seen in Exp 15 are caused by **double biasing**.
- We switched to Front Face Culling, which uses the object's own thickness as a geometric bias.
- However, we left the large shader biases (Exp 13) and Normal Offset (Exp 14) enabled.
- This combination pushes the shadow start point too far from the object base.
**Action:**
- Updated `shaders/fragment.wgsl`:
  - Disabled `normal_offset` (set to 0.0).
  - Reduced `base_bias` from `0.00005` to `0.000005` (10x reduction).
  - Reduced `slope_bias` max from `0.0005` to `0.00005` (10x reduction).
**Expected Result:**
- Shadows should re-attach to the base of objects.
- Acne should still be prevented by the Front Face Culling (geometric bias).

## Experiment 17: Hardware Depth Bias + Standard Culling
**Date:** 2024-05-24
**Hypothesis:** The "Front Face Culling" strategy (Exp 15) failed because the terrain's "back faces" (bottom of the chunk) are underground and do not cast shadows on the surface. This forced us to render Front Faces (Top Surface) anyway, leading to acne.
- The "Acne" persisted because we reduced the shader bias in Exp 16.
- The "Peter Panning" in Exp 15/16 was due to double-biasing or incorrect bias application.
- **Hardware Depth Bias** is the industry standard solution. It applies bias during rasterization (modifying the Z-buffer value), which is more robust and efficient than shader-side bias.
**Action:**
- Updated `moho_renderer/src/shadow.rs`:
  - Reverted `cull_mode` to `Some(wgpu::Face::Back)` (Standard Culling).
  - Enabled `depth_bias` in `DepthStencilState`:
    - `constant: 2` (Base bias units).
    - `slope_scale: 2.0` (Slope-dependent bias).
    - `clamp: 0.0` (No clamp).
- Updated `shaders/fragment.wgsl`:
  - Disabled manual shader bias (set to minimal epsilon `0.000005`).
  - Disabled normal offset.
**Expected Result:**
- Shadows should be correct (hills casting on ground).
- Acne should be eliminated by the Hardware Bias.
- Peter Panning should be minimal because the bias is applied to the caster's depth, not the receiver's position.

## Experiment 18: Fix Shadow Fade and PCSS Quality Scaling
**Date:** 2024-05-24
**Hypothesis:** 
1.  **Shadow Fade:** The previous logic faded *after* `shadow_distance` (where the map is invalid) and forced a dark value (`0.7`), creating a "dark circle" artifact around the player. It should fade *approaching`shadow_distance` and fade the *shadow result* to 1.0 (lit).
2.  **PCSS Quality:** The quality scaling used `proj_coords.z` (light space depth), which roughly maps to "altitude" (inverted) for a sun light. This meant shadows on the ground got lower quality than shadows on mountains, regardless of camera distance. It should use `view_depth` (distance from camera).
**Action:**
- Updated `shaders/fragment.wgsl`:
  - `calculate_light_shadow`:
    - Removed the "pre-check" fade logic.
    - Added post-sample fade logic: `if (view_depth > shadow_distance * 0.8) { mix(shadow, 1.0, fade_factor) }`.
  - `sample_light_shadow`:
    - Added `view_depth` parameter.
    - Changed `depth_fraction` calculation from `proj_coords.z` to `view_depth / shadow_distance`.
**Expected Result:**
- No "dark circle" artifact at the shadow edge.
- Smooth transition from shadowed to lit at the edge of the shadow distance.
- Consistent shadow quality based on distance from the camera.

## Final Resolution
**Date:** 2024-05-24
**Status:** RESOLVED

The "Shadow Leak" and "Missing Shadow" issues have been resolved. The system is now stable and produces high-quality shadows.

### Root Causes & Fixes
1.  **Flickering:** Caused by the shadow projection moving with the camera.
    -   **Fix:** Implemented **Texel Snapping** in `shadow.rs` to lock the projection to the world grid.
2.  **Missing Shadows (Z-Range):** Caused by WGPU's `0..1` Z-range vs Glam's `-1..1` projection.
    -   **Fix:** Applied a **Correction Matrix** (Z-remap) in `shadow.rs`.
3.  **Inverted Shadows (Underground Sun):** Caused by subtracting the light direction instead of adding it.
    -   **Fix:** Corrected `light_pos` calculation to `center + light_dir * dist`.
4.  **Shadow Acne (Banding):** Caused by insufficient bias for the large scene scale (~3000m).
    -   **Fix:** Switched to **Hardware Depth Bias** (`constant: 2`, `slope_scale: 2.0`) which is more robust than shader-side bias.
5.  **Dark Circle Artifact:** Caused by incorrect fade logic in the shader.
    -   **Fix:** Updated `fragment.wgsl` to fade the *result* to 1.0 approaching the shadow limit.
6.  **Inconsistent Quality:** Caused by PCSS scaling based on light-space depth (altitude).
    -   **Fix:** Updated PCSS to scale quality based on `view_depth` (distance from camera).

### Current Configuration
-   **Culling:** Standard Back-Face Culling (`wgpu::Face::Back`).
-   **Bias:** Hardware Bias (`constant: 2`, `slope_scale: 2.0`).
-   **Shader:** PCSS with distance-based quality scaling.
-   **Projection:** Orthographic with Texel Snapping and Z-Correction.

### Remaining Minor Artifacts
-   Some minor "Peter Panning" (detached shadows) may be visible on specific sharp geometry (as seen in the final screenshot). This is a trade-off with the Hardware Bias settings. It can be tuned further (e.g., reducing `slope_scale` to 1.5) during a future polish phase, but the current state is robust and performant.
