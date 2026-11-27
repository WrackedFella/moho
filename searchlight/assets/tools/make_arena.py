#!/usr/bin/env python3
"""Generates the FPS prototype's greybox levels (arena.glb, proving_ground.glb, parkour.glb) and rifle shot
(rifle_shot.wav).

Standard library only. Output is deterministic, so rerunning the script leaves the
committed files unchanged unless the layout below changes.

    python3 -I make_arena.py [output-directory]

The output directory defaults to the directory above this script (searchlight/assets).

Units are metres, Y up, the player stands at the player spawn facing +Z.

Markers are empty, named glTF nodes that the game reads by name (see searchlight::arena):

    player_spawn      exactly one
    dummy_NN          training dummy, a floor point
    enemy_spawn_NN    where an enemy starts, a floor point
    waypoint_NN       a patrol point, a floor point
    cover_NN          a floor point an enemy can hide at; its extras say which way the
                      hiding geometry lies
    control_NN        a floor point a usable post stands on; its extras name the `action`
                      (`next_wave`)
    menu_camera_NN    a view the main menu shows behind itself, optional: the node's translation
                      is the eye and its rotation turns +Z to the view direction (roll is
                      ignored); the views show in number order

A cover point sits COVER_DEPTH behind the rear face of a COVER_HEIGHT block. A ray at body
height (BODY_HEIGHT above the floor) coming from the block's side is stopped by the block; a
ray at eye height (EYE_HEIGHT) clears it. Replacing arena.glb from Blender is fine as long as
the markers (and the extras on cover points) stay.
"""

import json
import math
import random
import struct
import sys
import wave
from pathlib import Path

# Heights in metres above the floor. The player's capsule is 2.3 m tall.
BODY_HEIGHT = 1.15
EYE_HEIGHT = 1.85
COVER_HEIGHT = 1.4
COVER_DEPTH = 0.8

HALF_SIZE = 20.0
WALL_HEIGHT = 4.0
WALL_THICKNESS = 1.0
FLOOR_THICKNESS = 1.0

PLAYER_SPAWN = (0.0, 0.0, -17.0)
DUMMIES = [(-9.0, 6.0), (-3.0, 12.0), (5.0, 9.0), (11.0, 14.0)]
ENEMY_SPAWNS = [(-14.0, 15.0), (14.0, 16.0), (0.0, 19.0)]
WAYPOINTS = [(-12.0, -10.0), (12.0, -10.0), (10.0, 4.0), (0.0, 8.0), (-10.0, 2.0)]

# Usable posts: (x, z, action). This one is 1.5 m beside the player spawn: in reach at once, clear
# of the spawn's capsule.
CONTROLS = [(1.5, -17.0, "next_wave")]

# Menu camera views: (x, z, yaw degrees, pitch degrees down). Each stands at EYE_HEIGHT and looks
# across the cover lanes from a different side.
MENU_CAMERAS = [
    (-18.0, -18.0, 40.0, 8.0),
    (18.0, -18.0, -40.0, 8.0),
    (18.0, 10.0, -110.0, 8.0),
]

# Cover blocks: (centre x, centre z, size x, size z, direction from the cover point to the
# block). The cover point is COVER_DEPTH behind the face opposite that direction.
COVER = [
    (-6.0, -4.0, 3.0, 0.8, (0, -1)),
    (6.0, -4.0, 3.0, 0.8, (0, -1)),
    (14.0, 2.0, 0.8, 3.0, (-1, 0)),
    (-14.0, 8.0, 0.8, 3.0, (1, 0)),
    (0.0, 16.0, 4.0, 0.8, (0, -1)),
]

# Full-height walls that break line of sight (the cover blocks are hip-high): (centre x,
# centre z, size x, size z). They carry no cover marker; an enemy hiding behind one would have
# no shot.
SCREENS = [
    (3.5, 12.0, 6.0, 0.5),
    (-8.0, 15.0, 0.5, 6.0),
]

# (normal, u, v) with u x v = normal: corners (-u-v, +u-v, +u+v, -u+v) wind counter-clockwise
# seen from outside, as glTF front faces do.
FACES = [
    ((1, 0, 0), (0, 1, 0), (0, 0, 1)),
    ((-1, 0, 0), (0, 0, 1), (0, 1, 0)),
    ((0, 1, 0), (0, 0, 1), (1, 0, 0)),
    ((0, -1, 0), (1, 0, 0), (0, 0, 1)),
    ((0, 0, 1), (1, 0, 0), (0, 1, 0)),
    ((0, 0, -1), (0, 1, 0), (1, 0, 0)),
]

MATERIALS = {
    "floor": [0.55, 0.55, 0.57, 1.0],
    "wall": [0.4, 0.42, 0.46, 1.0],
    "cover": [0.5, 0.45, 0.35, 1.0],
}

# The proving ground adds the floor markings (distance strips, digits, drop area). The game
# draws meshes of material `mark` in a contrasting colour.
PROVING_MATERIALS = {**MATERIALS, "mark": [0.95, 0.9, 0.2, 1.0]}

# Proving ground, +Z downrange, firing line at z = 0 (the player spawn).
LANE_HALF_WIDTH = 15.0
LANE_Z_MIN = -15.0
LANE_Z_MAX = 215.0
LANE_WALL_HEIGHT = 2.0
DUMMY_DISTANCES = [25, 50, 100, 200]
COVER_HEIGHTS = [0.6, 1.0, 1.4]
COVER_X = [4.0, 8.0, 12.0]
COVER_Z = 2.0
COVER_WIDTH = 2.0
COVER_DEPTH_Z = 1.0
PLATFORM_HEIGHT = 3.0
PLATFORM_X = (-15.0, -5.0)
PLATFORM_Z = (-15.0, -9.0)
STAIR_RISE = 0.2
STAIR_RUN = 0.3
STAIR_Z = (-15.0, -13.0)
SLOPE_DEGREES = 30.0
SLOPE_Z = (-12.0, -10.0)
DROP_AREA_X = (-14.0, -6.0)
DROP_AREA_Z = (-8.0, -3.0)
MARK_HEIGHT = 0.01
STRIP_DEPTH = 0.5

# Seven-segment digits laid flat on the floor, readable from the firing line.
DIGIT_WIDTH = 1.2
DIGIT_HEIGHT = 2.4
DIGIT_STROKE = 0.25
DIGIT_GAP = 0.4
DIGIT_STRIP_GAP = 0.5
SEGMENTS = {
    "0": "abcdef", "1": "bc", "2": "abdeg", "3": "abcdg", "4": "bcfg",
    "5": "acdfg", "6": "acdefg", "7": "abc", "8": "abcdefg", "9": "abcdfg",
}


def box(centre, half):
    """Positions, normals and triangle indices of a closed box."""
    positions, normals, indices = [], [], []
    for normal, u, v in FACES:
        first = len(positions)
        face = [centre[i] + normal[i] * half[i] for i in range(3)]
        du = [u[i] * half[i] for i in range(3)]
        dv = [v[i] * half[i] for i in range(3)]
        for su, sv in ((-1, -1), (1, -1), (1, 1), (-1, 1)):
            positions.append(tuple(face[i] + su * du[i] + sv * dv[i] for i in range(3)))
            normals.append(normal)
        indices += [first, first + 1, first + 2, first, first + 2, first + 3]
    return positions, normals, indices


def solids():
    """Every solid as (name, material, centre, half extents)."""
    out = [
        (
            "floor",
            "floor",
            (0.0, -FLOOR_THICKNESS / 2, 0.0),
            (HALF_SIZE + WALL_THICKNESS, FLOOR_THICKNESS / 2, HALF_SIZE + WALL_THICKNESS),
        )
    ]
    wall_centre = HALF_SIZE + WALL_THICKNESS / 2
    wall_long = HALF_SIZE + WALL_THICKNESS
    wall_half_y = WALL_HEIGHT / 2
    thin = WALL_THICKNESS / 2
    for name, sign, along_x in (("east", 1, True), ("west", -1, True),
                                ("north", 1, False), ("south", -1, False)):
        side = sign * wall_centre
        if along_x:
            centre, half = (side, wall_half_y, 0.0), (thin, wall_half_y, wall_long)
        else:
            centre, half = (0.0, wall_half_y, side), (wall_long, wall_half_y, thin)
        out.append((f"wall_{name}", "wall", centre, half))
    for index, (x, z, size_x, size_z, _) in enumerate(COVER, start=1):
        out.append(
            (f"crate_{index:02d}", "cover", (x, COVER_HEIGHT / 2, z),
             (size_x / 2, COVER_HEIGHT / 2, size_z / 2))
        )
    for index, (x, z, size_x, size_z) in enumerate(SCREENS, start=1):
        out.append(
            (f"screen_{index:02d}", "wall", (x, WALL_HEIGHT / 2, z),
             (size_x / 2, WALL_HEIGHT / 2, size_z / 2))
        )
    return out


def cover_point(block):
    """The floor point COVER_DEPTH behind the face of `block` away from its direction."""
    x, z, size_x, size_z, (dx, dz) = block
    return (
        x - dx * (size_x / 2 + COVER_DEPTH),
        z - dz * (size_z / 2 + COVER_DEPTH),
    )


def direction_name(direction):
    return {(1, 0): "+x", (-1, 0): "-x", (0, 1): "+z", (0, -1): "-z"}[direction]


def markers():
    """Every marker as (name, translation, extras or None)."""
    out = [("player_spawn", PLAYER_SPAWN, None)]
    for kind, points in (("dummy", DUMMIES), ("enemy_spawn", ENEMY_SPAWNS),
                         ("waypoint", WAYPOINTS)):
        for index, (x, z) in enumerate(points, start=1):
            out.append((f"{kind}_{index:02d}", (x, 0.0, z), None))
    for index, block in enumerate(COVER, start=1):
        x, z = cover_point(block)
        out.append((f"cover_{index:02d}", (x, 0.0, z),
                    {"shielded_from": direction_name(block[4])}))
    for index, (x, z, action) in enumerate(CONTROLS, start=1):
        out.append((f"control_{index:02d}", (x, 0.0, z), {"action": action}))
    for index, (x, z, yaw, pitch) in enumerate(MENU_CAMERAS, start=1):
        out.append((f"menu_camera_{index:02d}", (x, EYE_HEIGHT, z), None,
                    look_rotation(yaw, pitch)))
    return out


def look_rotation(yaw_degrees, pitch_degrees):
    """The quaternion (x, y, z, w) that tips +Z down by the pitch, then turns it about Y."""
    half_yaw = math.radians(yaw_degrees) / 2
    half_pitch = math.radians(pitch_degrees) / 2
    sy, cy = math.sin(half_yaw), math.cos(half_yaw)
    sx, cx = math.sin(half_pitch), math.cos(half_pitch)
    return [cy * sx, sy * cx, -sy * sx, cy * cx]


def prism(profile, z_min, z_max):
    """A convex profile in the XY plane (counter-clockwise) extruded along Z."""
    positions, normals, indices = [], [], []

    def face(corners, normal):
        first = len(positions)
        positions.extend(corners)
        normals.extend([normal] * len(corners))
        for i in range(1, len(corners) - 1):
            indices.extend([first, first + i, first + i + 1])

    face([(x, y, z_max) for x, y in profile], (0, 0, 1))
    face([(x, y, z_min) for x, y in reversed(profile)], (0, 0, -1))
    for i, (px, py) in enumerate(profile):
        qx, qy = profile[(i + 1) % len(profile)]
        length = math.hypot(qx - px, qy - py)
        normal = ((qy - py) / length, -(qx - px) / length, 0.0)
        face([(px, py, z_min), (qx, qy, z_min), (qx, qy, z_max), (px, py, z_max)], normal)
    return positions, normals, indices


def block(name, material, x_range, y_range, z_range):
    """A box solid spanning the given (min, max) ranges."""
    centre = tuple((lo + hi) / 2 for lo, hi in (x_range, y_range, z_range))
    half = tuple((hi - lo) / 2 for lo, hi in (x_range, y_range, z_range))
    return (name, material, box(centre, half))


def digit_slabs(name, digit, centre_x, far_z, scale=1.0, material="mark"):
    """The slabs of one digit, `scale` times the standard size; screen right is world -X,
    screen up is world +Z."""
    w, h, s = DIGIT_WIDTH * scale, DIGIT_HEIGHT * scale, DIGIT_STROKE * scale
    upper = (h / 2 + s / 2, h - s)
    lower = (s, h / 2 - s / 2)
    # segment: (u range, v range)
    layout = {
        "a": ((-w / 2, w / 2), (h - s, h)),
        "g": ((-w / 2, w / 2), (h / 2 - s / 2, h / 2 + s / 2)),
        "d": ((-w / 2, w / 2), (0.0, s)),
        "f": ((-w / 2, -w / 2 + s), upper),
        "b": ((w / 2 - s, w / 2), upper),
        "e": ((-w / 2, -w / 2 + s), lower),
        "c": ((w / 2 - s, w / 2), lower),
    }
    z0 = far_z - h
    out = []
    for segment in SEGMENTS[digit]:
        (u0, u1), (v0, v1) = layout[segment]
        out.append(block(f"{name}_{segment}", material,
                         (centre_x - u1, centre_x - u0), (0.0, MARK_HEIGHT),
                         (z0 + v0, z0 + v1)))
    return out


def number_slabs(distance):
    """The distance written in digits, centred on x = 0, just short of its strip."""
    text = str(distance)
    total = len(text) * DIGIT_WIDTH + (len(text) - 1) * DIGIT_GAP
    far_z = distance - STRIP_DEPTH / 2 - DIGIT_STRIP_GAP
    out = []
    for index, digit in enumerate(text):
        screen_x = -total / 2 + DIGIT_WIDTH / 2 + index * (DIGIT_WIDTH + DIGIT_GAP)
        out.extend(digit_slabs(f"mark_{distance}_{index}", digit, -screen_x, far_z))
    return out


def proving_ground_solids():
    """Every solid of the proving ground as (name, material, geometry)."""
    half = LANE_HALF_WIDTH
    out = [
        block("floor", "floor", (-half - 1, half + 1), (-FLOOR_THICKNESS, 0.0),
              (LANE_Z_MIN - 1, LANE_Z_MAX + 1)),
        block("wall_east", "wall", (half, half + 1), (0.0, LANE_WALL_HEIGHT),
              (LANE_Z_MIN - 1, LANE_Z_MAX + 1)),
        block("wall_west", "wall", (-half - 1, -half), (0.0, LANE_WALL_HEIGHT),
              (LANE_Z_MIN - 1, LANE_Z_MAX + 1)),
        block("wall_north", "wall", (-half, half), (0.0, LANE_WALL_HEIGHT),
              (LANE_Z_MAX, LANE_Z_MAX + 1)),
        block("wall_south", "wall", (-half, half), (0.0, LANE_WALL_HEIGHT),
              (LANE_Z_MIN - 1, LANE_Z_MIN)),
    ]
    for height, x in zip(COVER_HEIGHTS, COVER_X):
        out.append(block(f"block_{height:.1f}".replace(".", "_"), "cover",
                         (x - COVER_WIDTH / 2, x + COVER_WIDTH / 2), (0.0, height),
                         (COVER_Z - COVER_DEPTH_Z / 2, COVER_Z + COVER_DEPTH_Z / 2)))
    out.append(block("platform", "cover", PLATFORM_X, (0.0, PLATFORM_HEIGHT), PLATFORM_Z))
    steps = round(PLATFORM_HEIGHT / STAIR_RISE)
    for step in range(1, steps):
        x_far = PLATFORM_X[1] + (steps - step) * STAIR_RUN
        out.append(block(f"stair_{step:02d}", "cover", (x_far - STAIR_RUN, x_far),
                         (0.0, step * STAIR_RISE), STAIR_Z))
    run = PLATFORM_HEIGHT / math.tan(math.radians(SLOPE_DEGREES))
    x_top = PLATFORM_X[1]
    profile = [(x_top, 0.0), (x_top + run, 0.0), (x_top, PLATFORM_HEIGHT)]
    out.append(("slope", "cover", prism(profile, *SLOPE_Z)))
    out.append(block("drop_area", "mark", DROP_AREA_X, (0.0, MARK_HEIGHT), DROP_AREA_Z))
    for distance in DUMMY_DISTANCES:
        out.append(block(f"strip_{distance}", "mark", (-half, half), (0.0, MARK_HEIGHT),
                         (distance - STRIP_DEPTH / 2, distance + STRIP_DEPTH / 2)))
        out.extend(number_slabs(distance))
    return out


def proving_ground_markers():
    """The proving ground's markers: no enemy spawns, waypoints or covers."""
    out = [("player_spawn", (0.0, 0.0, 0.0), None)]
    for index, distance in enumerate(DUMMY_DISTANCES, start=1):
        out.append((f"dummy_{index:02d}", (0.0, 0.0, float(distance)), None))
    return out


# Parkour level, +Z the direction of travel. Everything stands on y = 0 floors.
# Gap ladder: one 2 m lane per width, side by side along X, each a run-up platform, a gap of
# the lane's width over a pit, and a landing platform. The widths are labelled in decimetres.
GAP_WIDTHS = [0.5 * n for n in range(1, 11)]
# What clears each gap (the smallest movement): `stand` (a jump from standing), `run` (a
# jump out of a run-up at walking speed) or `none`. Set from what the headless jump tests
# measure with the shipped movement tuning; regenerate these when that tuning changes.
GAP_CLEARS = [
    "stand", "run", "run", "run", "run", "run", "run", "run", "run", "none",
]
LANE_WIDTH = 2.0
RUN_UP_LENGTH = 6.0
LANDING_LENGTH = 6.0
PIT_DEPTH = 2.0
PIT_FLOOR_THICKNESS = 1.0
APRON_Z = (-12.0, -RUN_UP_LENGTH)
WALKWAY_X = (10.0, 15.0)
LABEL_SCALE = 0.4
LADDER_LABEL_FAR_Z = -1.2

# Course: one 3 m lane per piece, past the ladder. Each piece starts at COURSE_FOOT_Z, with a
# marker at its foot, and rises to a top platform COURSE_TOP_DEPTH deep.
# (kind, value, passable): steps and ledges in metres, slopes in degrees. `passable` is
# `walk`, `jump` or `none`, set from what the headless climb tests measure.
COURSE = [
    ("step", 0.3, "walk"),
    ("step", 0.5, "walk"),
    ("step", 0.6, "jump"),
    ("ledge", 1.0, "jump"),
    ("ledge", 1.2, "jump"),
    ("ledge", 1.4, "jump"),
    ("slope", 30.0, "walk"),
    ("slope", 45.0, "walk"),
    ("slope", 50.0, "jump"),
]
COURSE_LANE_WIDTH = 3.0
COURSE_FLOOR_Z = (11.0, 20.0)
COURSE_FOOT_Z = 20.0
COURSE_TOP_DEPTH = 6.0
RAMP_HEIGHT = 2.0
COURSE_HALF_WIDTH = 15.0
PARKOUR_SPAWN = (0.0, 0.0, -9.0)

PARKOUR_MATERIALS = {**PROVING_MATERIALS, "mark_run": [0.3, 0.85, 0.35, 1.0]}


def lane_centre(index, count, width):
    return (index - (count - 1) / 2) * width


def label_slabs(name, text, centre_x, far_z, material="mark"):
    """`text` in small block digits centred on `centre_x`, its far edge at `far_z`."""
    digit_width = DIGIT_WIDTH * LABEL_SCALE
    gap = DIGIT_GAP * LABEL_SCALE
    total = len(text) * digit_width + (len(text) - 1) * gap
    out = []
    for index, digit in enumerate(text):
        screen_x = -total / 2 + digit_width / 2 + index * (digit_width + gap)
        out.extend(digit_slabs(f"{name}_{index}", digit, centre_x - screen_x, far_z,
                               LABEL_SCALE, material))
    return out


def ramp(name, material, x_range, height, run, z_start):
    """A wedge rising from y = 0 at `z_start` to `height` at `z_start + run`, across x_range."""
    profile = [(z_start, 0.0), (z_start + run, 0.0), (z_start + run, height)]
    positions, normals, indices = prism(profile, *x_range)
    # The profile's X is world Z and its extrusion is world X: a reflection, so the
    # triangles are reversed to stay counter-clockwise from outside.
    positions = [(pz, py, px) for px, py, pz in positions]
    normals = [(nz, ny, nx) for nx, ny, nz in normals]
    flipped = []
    for i in range(0, len(indices), 3):
        a, b, c = indices[i:i + 3]
        flipped += [a, c, b]
    return (name, material, (positions, normals, flipped))


def parkour_solids():
    """Every solid of the parkour level as (name, material, geometry)."""
    ladder_half = len(GAP_WIDTHS) * LANE_WIDTH / 2
    out = [
        block("apron", "floor", (-ladder_half, ladder_half), (-FLOOR_THICKNESS, 0.0), APRON_Z),
        block("walkway", "floor", WALKWAY_X, (-FLOOR_THICKNESS, 0.0),
              (APRON_Z[0], COURSE_FLOOR_Z[0])),
        block("course_floor", "floor", (-COURSE_HALF_WIDTH, COURSE_HALF_WIDTH),
              (-FLOOR_THICKNESS, 0.0), COURSE_FLOOR_Z),
    ]
    for index, width in enumerate(GAP_WIDTHS):
        centre = lane_centre(index, len(GAP_WIDTHS), LANE_WIDTH)
        lane = (centre - LANE_WIDTH / 2, centre + LANE_WIDTH / 2)
        number = index + 1
        out.append(block(f"pit_{number:02d}", "wall",
                         lane, (-PIT_DEPTH - PIT_FLOOR_THICKNESS, -PIT_DEPTH),
                         (-RUN_UP_LENGTH, width + LANDING_LENGTH)))
        out.append(block(f"runup_{number:02d}", "floor", lane, (-PIT_DEPTH, 0.0),
                         (-RUN_UP_LENGTH, 0.0)))
        out.append(block(f"landing_{number:02d}", "floor", lane, (-PIT_DEPTH, 0.0),
                         (width, width + LANDING_LENGTH)))
        decimetres = str(round(width * 10))
        # Raised digits across a lane stop a capsule, so the clears colour is in the digits,
        # which are sparse, not in a strip.
        colour = "mark_run" if GAP_CLEARS[index] == "run" else "mark"
        out.extend(label_slabs(f"label_{number:02d}", decimetres, centre,
                               LADDER_LABEL_FAR_Z, colour))
    for index, (kind, value, _) in enumerate(COURSE):
        centre = lane_centre(index, len(COURSE), COURSE_LANE_WIDTH)
        lane = (centre - COURSE_LANE_WIDTH / 2, centre + COURSE_LANE_WIDTH / 2)
        number = index + 1
        if kind == "slope":
            run = RAMP_HEIGHT / math.tan(math.radians(value))
            out.append(ramp(f"piece_{number:02d}", "cover", lane, RAMP_HEIGHT, run,
                            COURSE_FOOT_Z))
            top_start, top = COURSE_FOOT_Z + run, RAMP_HEIGHT
            text = str(round(value))
        else:
            top_start, top = COURSE_FOOT_Z, value
            out.append(block(f"piece_{number:02d}", "cover", lane, (0.0, top),
                             (COURSE_FOOT_Z, COURSE_FOOT_Z + COURSE_TOP_DEPTH)))
            text = str(round(value * 100))
        if kind == "slope":
            out.append(block(f"top_{number:02d}", "cover", lane, (0.0, top),
                             (top_start, top_start + COURSE_TOP_DEPTH)))
        out.extend(label_slabs(f"course_label_{number:02d}", text, centre,
                               COURSE_FOOT_Z - 1.0))
    return out


def parkour_markers():
    """The parkour level's markers: a spawn, a `gap_NN` at each take-off edge and a
    `course_NN` at the foot of each piece, all on their floors and facing +Z."""
    out = [("player_spawn", PARKOUR_SPAWN, None)]
    for index, (width, clears) in enumerate(zip(GAP_WIDTHS, GAP_CLEARS), start=1):
        centre = lane_centre(index - 1, len(GAP_WIDTHS), LANE_WIDTH)
        out.append((f"gap_{index:02d}", (centre, 0.0, 0.0),
                    {"width": width, "clears": clears}))
    for index, (kind, value, passable) in enumerate(COURSE, start=1):
        centre = lane_centre(index - 1, len(COURSE), COURSE_LANE_WIDTH)
        out.append((f"course_{index:02d}", (centre, 0.0, COURSE_FOOT_Z),
                    {"kind": kind, "value": value, "passable": passable}))
    return out


def build_glb(solid_list, marker_list, materials):
    """A glb of `solid_list` ((name, material, geometry)) and `marker_list`."""
    document = {
        "asset": {"version": "2.0", "generator": "searchlight/assets/tools/make_arena.py"},
        "scene": 0,
        "scenes": [{"nodes": []}],
        "nodes": [],
        "meshes": [],
        "materials": [
            {"name": name, "pbrMetallicRoughness": {"baseColorFactor": colour,
                                                    "metallicFactor": 0.0,
                                                    "roughnessFactor": 1.0}}
            for name, colour in materials.items()
        ],
        "accessors": [],
        "bufferViews": [],
        "buffers": [],
    }
    material_index = {name: i for i, name in enumerate(materials)}
    blob = bytearray()

    def add_view(data, target):
        document["bufferViews"].append(
            {"buffer": 0, "byteOffset": len(blob), "byteLength": len(data), "target": target}
        )
        blob.extend(data)
        while len(blob) % 4:
            blob.append(0)
        return len(document["bufferViews"]) - 1

    def add_accessor(view, component, count, kind, bounds=None):
        accessor = {"bufferView": view, "componentType": component, "count": count,
                    "type": kind}
        if bounds:
            accessor["min"], accessor["max"] = bounds
        document["accessors"].append(accessor)
        return len(document["accessors"]) - 1

    for name, material, (positions, normals, indices) in solid_list:
        flat = [c for p in positions for c in p]
        bounds = ([min(p[i] for p in positions) for i in range(3)],
                  [max(p[i] for p in positions) for i in range(3)])
        position_view = add_view(struct.pack(f"<{len(flat)}f", *flat), 34962)
        normal_view = add_view(
            struct.pack(f"<{len(flat)}f", *[c for n in normals for c in n]), 34962)
        index_view = add_view(struct.pack(f"<{len(indices)}H", *indices), 34963)
        document["meshes"].append({
            "name": name,
            "primitives": [{
                "attributes": {
                    "POSITION": add_accessor(position_view, 5126, len(positions), "VEC3", bounds),
                    "NORMAL": add_accessor(normal_view, 5126, len(normals), "VEC3"),
                },
                "indices": add_accessor(index_view, 5123, len(indices), "SCALAR"),
                "material": material_index[material],
            }],
        })
        document["nodes"].append({"name": f"{name}_node", "mesh": len(document["meshes"]) - 1})

    for name, translation, extras, *rotation in marker_list:
        node = {"name": name, "translation": list(translation)}
        if rotation:
            node["rotation"] = rotation[0]
        if extras:
            node["extras"] = extras
        document["nodes"].append(node)

    document["scenes"][0]["nodes"] = list(range(len(document["nodes"])))
    document["buffers"].append({"byteLength": len(blob)})

    json_chunk = json.dumps(document, separators=(",", ":")).encode()
    json_chunk += b" " * (-len(json_chunk) % 4)
    total = 12 + 8 + len(json_chunk) + 8 + len(blob)
    return b"".join([
        struct.pack("<4sII", b"glTF", 2, total),
        struct.pack("<I4s", len(json_chunk), b"JSON"), json_chunk,
        struct.pack("<I4s", len(blob), b"BIN\0"), bytes(blob),
    ])


def build_shot(path):
    """A short gunshot: a noise crack over a falling low thump, both decaying fast."""
    rate = 22050
    length = 0.35
    rng = random.Random(7)
    samples = []
    phase = 0.0
    for n in range(int(rate * length)):
        t = n / rate
        crack = rng.uniform(-1.0, 1.0) * math.exp(-t * 40.0)
        frequency = 40.0 + 110.0 * math.exp(-t * 25.0)
        phase += 2.0 * math.pi * frequency / rate
        thump = math.sin(phase) * math.exp(-t * 14.0)
        value = max(-1.0, min(1.0, 0.7 * crack + 0.6 * thump))
        samples.append(int(value * 32767 * 0.9))
    with wave.open(str(path), "wb") as out:
        out.setnchannels(1)
        out.setsampwidth(2)
        out.setframerate(rate)
        out.writeframes(struct.pack(f"<{len(samples)}h", *samples))


def main():
    target = Path(sys.argv[1]) if len(sys.argv) > 1 else Path(__file__).resolve().parent.parent
    target.mkdir(parents=True, exist_ok=True)
    arena = [(name, material, box(centre, half)) for name, material, centre, half in solids()]
    (target / "arena.glb").write_bytes(build_glb(arena, markers(), MATERIALS))
    (target / "proving_ground.glb").write_bytes(
        build_glb(proving_ground_solids(), proving_ground_markers(), PROVING_MATERIALS))
    (target / "parkour.glb").write_bytes(
        build_glb(parkour_solids(), parkour_markers(), PARKOUR_MATERIALS))
    build_shot(target / "rifle_shot.wav")


if __name__ == "__main__":
    main()
