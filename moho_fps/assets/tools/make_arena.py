#!/usr/bin/env python3
"""Generates the FPS prototype's greybox arena (arena.glb) and rifle shot (rifle_shot.wav).

Standard library only. Output is deterministic, so rerunning the script leaves the
committed files unchanged unless the layout below changes.

    python3 -I make_arena.py [output-directory]

The output directory defaults to the directory above this script (moho_fps/assets).

Units are metres, Y up, the player stands at the player spawn facing +Z.

Markers are empty, named glTF nodes that the game reads by name (see moho_fps::arena):

    player_spawn      exactly one
    dummy_NN          training dummy, a floor point
    enemy_spawn_NN    where an enemy starts, a floor point
    waypoint_NN       a patrol point, a floor point
    cover_NN          a floor point an enemy can hide at; its extras say which way the
                      hiding geometry lies

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

# Cover blocks: (centre x, centre z, size x, size z, direction from the cover point to the
# block). The cover point is COVER_DEPTH behind the face opposite that direction.
COVER = [
    (-6.0, -4.0, 3.0, 0.8, (0, -1)),
    (6.0, -4.0, 3.0, 0.8, (0, -1)),
    (14.0, 2.0, 0.8, 3.0, (-1, 0)),
    (-14.0, 8.0, 0.8, 3.0, (1, 0)),
    (0.0, 16.0, 4.0, 0.8, (0, -1)),
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
    return out


def build_glb():
    document = {
        "asset": {"version": "2.0", "generator": "moho_fps/assets/tools/make_arena.py"},
        "scene": 0,
        "scenes": [{"nodes": []}],
        "nodes": [],
        "meshes": [],
        "materials": [
            {"name": name, "pbrMetallicRoughness": {"baseColorFactor": colour,
                                                    "metallicFactor": 0.0,
                                                    "roughnessFactor": 1.0}}
            for name, colour in MATERIALS.items()
        ],
        "accessors": [],
        "bufferViews": [],
        "buffers": [],
    }
    material_index = {name: i for i, name in enumerate(MATERIALS)}
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

    for name, material, centre, half in solids():
        positions, normals, indices = box(centre, half)
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

    for name, translation, extras in markers():
        node = {"name": name, "translation": list(translation)}
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
    (target / "arena.glb").write_bytes(build_glb())
    build_shot(target / "rifle_shot.wav")


if __name__ == "__main__":
    main()
