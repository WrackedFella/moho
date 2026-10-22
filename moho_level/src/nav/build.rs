//! Navmesh generation: world meshes through `rerecast`, converted to plain polygons.

use std::fmt::Display;

use glam::Vec3;
use moho_render_api::WorldMesh;
use rerecast::{ConfigBuilder, HeightfieldBuilder, PolygonNavmesh, TriMesh};

use super::{AgentProfile, NavBuildError, Polygon};

/// The most heightfield columns one build may allocate (2048 × 2048). A tiny agent over a
/// large level would otherwise take gigabytes and minutes.
const MAX_COLUMNS: u64 = 1 << 22;

/// Columns of a heightfield `width` by `depth` cells, rounded as rerecast rounds them.
fn columns(width: f32, depth: f32) -> u64 {
    (width + 0.5) as u64 * (depth + 0.5) as u64
}

fn fits(columns: u64) -> bool {
    columns <= MAX_COLUMNS
}

fn fail(stage: &str, error: impl Display) -> NavBuildError {
    NavBuildError(format!("{stage}: {error}"))
}

/// The meshes' triangles as one `rerecast` input, walkable by slope. `None` when there are none.
fn collect_triangles<'a>(
    meshes: impl IntoIterator<Item = &'a WorldMesh>,
    max_slope: f32,
) -> Result<Option<TriMesh>, NavBuildError> {
    let mut trimesh = TriMesh::default();
    for mesh in meshes {
        let base = u32::try_from(trimesh.vertices.len())
            .map_err(|_| fail("input", "more than u32::MAX vertices"))?;
        for &position in mesh.positions() {
            trimesh.vertices.push(position.into());
        }
        for tri in mesh.indices().as_chunks::<3>().0 {
            trimesh
                .indices
                .push([base + tri[0], base + tri[1], base + tri[2]].into());
            trimesh.area_types.push(rerecast::AreaType::default());
        }
    }
    if trimesh.indices.is_empty() {
        return Ok(None);
    }
    trimesh.mark_walkable_triangles(max_slope);
    Ok(Some(trimesh))
}

fn validate(profile: AgentProfile) -> Result<(), NavBuildError> {
    let positive = |v: f32| v.is_finite() && v > 0.0;
    let slope_ok = profile.max_slope.is_finite()
        && (0.0..std::f32::consts::FRAC_PI_2).contains(&profile.max_slope);
    if positive(profile.radius)
        && positive(profile.height)
        && profile.max_climb.is_finite()
        && profile.max_climb >= 0.0
        && slope_ok
    {
        Ok(())
    } else {
        Err(fail(
            "profile",
            format!("{profile:?} is not a walkable agent"),
        ))
    }
}

/// Builds the walkable polygons for `profile`; empty when the meshes have no triangles.
pub(super) fn polygons<'a>(
    meshes: impl IntoIterator<Item = &'a WorldMesh>,
    profile: AgentProfile,
) -> Result<Vec<Polygon>, NavBuildError> {
    validate(profile)?;
    let Some(trimesh) = collect_triangles(meshes, profile.max_slope)? else {
        return Ok(Vec::new());
    };
    let aabb = trimesh
        .compute_aabb()
        .ok_or_else(|| fail("input", "triangles have no bounds"))?;
    let config = ConfigBuilder {
        agent_radius: profile.radius,
        agent_height: profile.height,
        walkable_climb: profile.max_climb,
        ..Default::default()
    }
    .build();

    let width = (aabb.max.x - aabb.min.x) / config.cell_size;
    let depth = (aabb.max.z - aabb.min.z) / config.cell_size;
    if !fits(columns(width, depth)) {
        return Err(fail(
            "profile",
            format!("{profile:?} needs more than {MAX_COLUMNS} cells over these meshes"),
        ));
    }

    let mut heightfield = HeightfieldBuilder {
        aabb,
        cell_size: config.cell_size,
        cell_height: config.cell_height,
    }
    .build()
    .map_err(|e| fail("heightfield", e))?;
    heightfield
        .rasterize_triangles(&trimesh, config.walkable_climb)
        .map_err(|e| fail("rasterize", e))?;
    heightfield.filter_low_hanging_walkable_obstacles(config.walkable_climb);
    heightfield.filter_ledge_spans(config.walkable_height, config.walkable_climb);
    heightfield.filter_walkable_low_height_spans(config.walkable_height);

    let mut compact = heightfield
        .into_compact(config.walkable_height, config.walkable_climb)
        .map_err(|e| fail("compact heightfield", e))?;
    compact.erode_walkable_area(config.walkable_radius);
    compact.build_distance_field();
    // The border strip only serves tiled builds; this one covers the whole level.
    compact
        .build_regions(0, config.min_region_area, config.merge_region_area)
        .map_err(|e| fail("regions", e))?;
    let contours = compact.build_contours(
        config.max_simplification_error,
        config.max_edge_len,
        config.contour_flags,
    );
    let mesh = contours
        .into_polygon_mesh(config.max_vertices_per_polygon)
        .map_err(|e| fail("polygons", e))?;
    Ok(convert(&mesh))
}

/// Our own plain polygons, with vertices in world space and a neighbour per edge.
fn convert(mesh: &PolygonNavmesh) -> Vec<Polygon> {
    let origin = Vec3::from(mesh.aabb.min.to_array());
    let cell = Vec3::new(mesh.cell_size, mesh.cell_height, mesh.cell_size);
    let world: Vec<Vec3> = mesh
        .vertices
        .iter()
        .map(|v| origin + Vec3::new(f32::from(v.x), f32::from(v.y), f32::from(v.z)) * cell)
        .collect();
    let per_polygon = usize::from(mesh.max_vertices_per_polygon);
    mesh.polygons()
        .zip(mesh.polygon_neighbors.chunks_exact(per_polygon))
        .map(|(indices, neighbours)| {
            let mut vertices: Vec<Vec3> = indices.map(|i| world[usize::from(i)]).collect();
            let mut neighbours: Vec<Option<usize>> = neighbours
                .iter()
                .take(vertices.len())
                .map(|&n| (n != PolygonNavmesh::NO_CONNECTION).then_some(usize::from(n)))
                .collect();
            // `rerecast` winds every polygon clockwise in our x-right, z-up convention.
            debug_assert!(
                signed_area(&vertices) < 0.0,
                "rerecast polygon is not clockwise"
            );
            // Reversed, edge `k` is old edge `n - 2 - k` walked backwards.
            vertices.reverse();
            neighbours.reverse();
            neighbours.rotate_left(1);
            Polygon {
                vertices,
                neighbours,
            }
        })
        .collect()
}

/// Twice the signed area on the ground plane (x right, z up); positive when counter-clockwise.
fn signed_area(vertices: &[Vec3]) -> f32 {
    let next = vertices.iter().cycle().skip(1);
    vertices
        .iter()
        .zip(next)
        .map(|(a, b)| a.x * b.z - b.x * a.z)
        .sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn columns_round_each_side_to_the_nearest_cell() {
        assert_eq!(columns(10.0, 20.0), 200);
        assert_eq!(columns(10.4, 19.6), 200);
        assert_eq!(columns(10.6, 0.4), 0);
    }

    #[test]
    fn a_field_fits_up_to_the_column_limit() {
        assert!(fits(MAX_COLUMNS));
        assert!(!fits(MAX_COLUMNS + 1));
    }
}
