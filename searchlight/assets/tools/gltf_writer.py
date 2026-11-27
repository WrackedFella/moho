"""A minimal glTF 2.0 binary (.glb) writer for generated placeholders, standard library only.

The same inputs give the same bytes: no timestamps, fixed float packing, insertion-ordered
JSON. Geometry is boxes, prisms and domes in flat material colours; characters are rigidly
skinned (every vertex weighted 1.0 to one joint).
"""

from __future__ import annotations

import json
import math
import struct
from dataclasses import dataclass, field

FLOAT, UBYTE, USHORT, UINT = 5126, 5121, 5123, 5125
ARRAY_BUFFER, ELEMENT_ARRAY_BUFFER = 34962, 34963
GENERATOR = "searchlight-assets source/generators"


# --- vector and quaternion helpers (quaternions are x, y, z, w as in glTF) ---

def add(a, b): return tuple(x + y for x, y in zip(a, b))
def sub(a, b): return tuple(x - y for x, y in zip(a, b))
def scale(a, s): return tuple(x * s for x in a)
def dot(a, b): return sum(x * y for x, y in zip(a, b))
def cross(a, b): return (a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0])
def length(a): return math.sqrt(dot(a, a))
def normalize(a): return scale(a, 1.0 / length(a))


def quat_axis_angle(axis, degrees):
    axis = normalize(axis)
    half = math.radians(degrees) / 2.0
    s = math.sin(half)
    return (axis[0] * s, axis[1] * s, axis[2] * s, math.cos(half))


def quat_mul(a, b):
    ax, ay, az, aw = a
    bx, by, bz, bw = b
    return (
        aw * bx + ax * bw + ay * bz - az * by,
        aw * by - ax * bz + ay * bw + az * bx,
        aw * bz + ax * by - ay * bx + az * bw,
        aw * bw - ax * bx - ay * by - az * bz,
    )


def quat_conj(q): return (-q[0], -q[1], -q[2], q[3])


def quat_rotate(q, v):
    p = quat_mul(quat_mul(q, (v[0], v[1], v[2], 0.0)), quat_conj(q))
    return p[:3]


def quat_between(a, b):
    """Shortest-arc rotation taking direction a to direction b."""
    a, b = normalize(a), normalize(b)
    c = dot(a, b)
    if c < -0.999999:
        axis = cross((1.0, 0.0, 0.0), a)
        if length(axis) < 1e-6:
            axis = cross((0.0, 0.0, 1.0), a)
        return quat_axis_angle(axis, 180.0)
    v = cross(a, b)
    q = (v[0], v[1], v[2], 1.0 + c)
    n = math.sqrt(dot(q, q))
    return tuple(x / n for x in q)


def euler_x(deg): return quat_axis_angle((1.0, 0.0, 0.0), deg)
def euler_y(deg): return quat_axis_angle((0.0, 1.0, 0.0), deg)
def euler_z(deg): return quat_axis_angle((0.0, 0.0, 1.0), deg)


IDENTITY = (0.0, 0.0, 0.0, 1.0)


# --- geometry ---

@dataclass
class Geometry:
    """Flat-shaded triangles: positions, normals, a joint per vertex, indices."""

    positions: list = field(default_factory=list)
    normals: list = field(default_factory=list)
    joints: list = field(default_factory=list)
    indices: list = field(default_factory=list)

    def quad(self, corners, joint=0):
        a, b, c, d = corners
        n = normalize(cross(sub(b, a), sub(c, a)))
        base = len(self.positions)
        for p in corners:
            self.positions.append(p)
            self.normals.append(n)
            self.joints.append(joint)
        self.indices += [base, base + 1, base + 2, base, base + 2, base + 3]

    def tri(self, a, b, c, joint=0):
        n = normalize(cross(sub(b, a), sub(c, a)))
        base = len(self.positions)
        for p in (a, b, c):
            self.positions.append(p)
            self.normals.append(n)
            self.joints.append(joint)
        self.indices += [base, base + 1, base + 2]

    def box(self, centre, size, joint=0, rotation=IDENTITY):
        """An axis-aligned box (optionally rotated about its centre), outward faces CCW."""
        hx, hy, hz = (s / 2.0 for s in size)

        def p(x, y, z):
            return add(centre, quat_rotate(rotation, (x * hx, y * hy, z * hz)))

        faces = (
            ((1, -1, -1), (1, 1, -1), (1, 1, 1), (1, -1, 1)),      # +x
            ((-1, -1, 1), (-1, 1, 1), (-1, 1, -1), (-1, -1, -1)),  # -x
            ((-1, 1, -1), (-1, 1, 1), (1, 1, 1), (1, 1, -1)),      # +y
            ((-1, -1, 1), (-1, -1, -1), (1, -1, -1), (1, -1, 1)),  # -y
            ((-1, -1, 1), (1, -1, 1), (1, 1, 1), (-1, 1, 1)),      # +z
            ((1, -1, -1), (-1, -1, -1), (-1, 1, -1), (1, 1, -1)),  # -z
        )
        for face in faces:
            self.quad([p(*c) for c in face], joint)

    def dome(self, centre, radius, segments=10, rings=4, min_elevation=0.0, joint=0):
        """An upper half (or cap) sphere, flat-shaded, open at the bottom ring."""

        def point(ring, seg):
            elev = min_elevation + (math.pi / 2 - min_elevation) * ring / rings
            az = 2 * math.pi * seg / segments
            return add(centre, (radius * math.cos(elev) * math.sin(az), radius * math.sin(elev),
                                radius * math.cos(elev) * math.cos(az)))

        for r in range(rings):
            for s in range(segments):
                a, b = point(r, s), point(r, s + 1)
                c, d = point(r + 1, s + 1), point(r + 1, s)
                if r == rings - 1:
                    self.tri(a, b, c, joint)
                else:
                    self.quad([a, b, c, d], joint)

    def bounds(self):
        xs, ys, zs = zip(*self.positions)
        return (min(xs), min(ys), min(zs)), (max(xs), max(ys), max(zs))


# --- document ---

def _r(x):
    """Rounds JSON floats so the text is stable and short."""
    return round(float(x), 6)


def _f32(x):
    """The value as the binary buffer stores it, so accessor bounds match the data exactly."""
    return struct.unpack("<f", struct.pack("<f", x))[0]


class Document:
    def __init__(self, scene_name):
        self.json = {
            "asset": {"version": "2.0", "generator": GENERATOR},
            "scene": 0,
            "scenes": [{"name": scene_name, "nodes": []}],
            "nodes": [], "meshes": [], "materials": [], "accessors": [], "bufferViews": [],
            "buffers": [{"byteLength": 0}],
        }
        self.bin = bytearray()
        self._materials = {}

    # buffers
    def _view(self, data: bytes, target=None):
        while len(self.bin) % 4:
            self.bin.append(0)
        view = {"buffer": 0, "byteOffset": len(self.bin), "byteLength": len(data)}
        if target:
            view["target"] = target
        self.bin += data
        self.json["bufferViews"].append(view)
        return len(self.json["bufferViews"]) - 1

    def _accessor(self, view, ctype, count, atype, minmax=None, normalized=False):
        acc = {"bufferView": view, "componentType": ctype, "count": count, "type": atype}
        if normalized:
            acc["normalized"] = True
        if minmax:
            acc["min"], acc["max"] = [[_f32(x) for x in m] for m in minmax]
        self.json["accessors"].append(acc)
        return len(self.json["accessors"]) - 1

    def floats(self, rows, atype, target=None, minmax=False):
        flat = [round(float(x), 6) + 0.0 for row in rows for x in (row if isinstance(row, (tuple, list)) else (row,))]
        view = self._view(struct.pack(f"<{len(flat)}f", *flat), target)
        mm = None
        if minmax:
            width = len(rows[0]) if isinstance(rows[0], (tuple, list)) else 1
            cols = list(zip(*[tuple(round(float(x), 6) + 0.0 for x in (r if isinstance(r, (tuple, list)) else (r,)))
                              for r in rows]))
            mm = ([min(c) for c in cols], [max(c) for c in cols]) if width else None
        return self._accessor(view, FLOAT, len(rows), atype, mm)

    # materials
    def material(self, name, rgb, metallic=0.0, roughness=0.85):
        if name not in self._materials:
            self.json["materials"].append({
                "name": name,
                "pbrMetallicRoughness": {
                    "baseColorFactor": [_r(rgb[0]), _r(rgb[1]), _r(rgb[2]), 1.0],
                    "metallicFactor": _r(metallic), "roughnessFactor": _r(roughness),
                },
            })
            self._materials[name] = len(self.json["materials"]) - 1
        return self._materials[name]

    # meshes
    def mesh(self, name, parts, skinned=False):
        """parts: list of (Geometry, material index); one primitive each."""
        prims = []
        for geo, mat in parts:
            pos = self.floats(geo.positions, "VEC3", ARRAY_BUFFER, minmax=True)
            nrm = self.floats(geo.normals, "VEC3", ARRAY_BUFFER)
            attrs = {"POSITION": pos, "NORMAL": nrm}
            if skinned:
                jdata = b"".join(struct.pack("<4B", j, 0, 0, 0) for j in geo.joints)
                attrs["JOINTS_0"] = self._accessor(self._view(jdata, ARRAY_BUFFER), UBYTE, len(geo.joints), "VEC4")
                attrs["WEIGHTS_0"] = self.floats([(1.0, 0.0, 0.0, 0.0)] * len(geo.joints), "VEC4", ARRAY_BUFFER)
            idx = self._view(struct.pack(f"<{len(geo.indices)}H", *geo.indices), ELEMENT_ARRAY_BUFFER)
            if len(geo.positions) > 65535:
                raise ValueError(f"{name}: too many vertices for 16-bit indices")
            prims.append({"attributes": attrs, "indices": self._accessor(idx, USHORT, len(geo.indices), "SCALAR"),
                          "material": mat})
        self.json["meshes"].append({"name": name, "primitives": prims})
        return len(self.json["meshes"]) - 1

    # nodes
    def node(self, name, parent=None, translation=None, rotation=None, mesh=None, extras=None, scale=None):
        n = {"name": name}
        if translation and any(abs(x) > 1e-9 for x in translation):
            n["translation"] = [_r(x) for x in translation]
        if rotation and rotation != IDENTITY:
            n["rotation"] = [_r(x) for x in rotation]
        if scale:
            n["scale"] = [_r(x) for x in scale]
        if mesh is not None:
            n["mesh"] = mesh
        if extras:
            n["extras"] = extras
        self.json["nodes"].append(n)
        index = len(self.json["nodes"]) - 1
        if parent is None:
            self.json["scenes"][0]["nodes"].append(index)
        else:
            self.json["nodes"][parent].setdefault("children", []).append(index)
        return index

    def skin(self, name, joints, inverse_binds, skeleton):
        mats = [x for m in inverse_binds for x in m]
        view = self._view(struct.pack(f"<{len(mats)}f", *mats))
        acc = self._accessor(view, FLOAT, len(inverse_binds), "MAT4")
        self.json.setdefault("skins", []).append(
            {"name": name, "joints": joints, "inverseBindMatrices": acc, "skeleton": skeleton})
        return len(self.json["skins"]) - 1

    def animation(self, name, channels):
        """channels: list of (node, path, times, values); linear interpolation."""
        samplers, chans = [], []
        for node, path, times, values in channels:
            t = self.floats(list(times), "SCALAR", minmax=True)
            v = self.floats(list(values), "VEC4" if path == "rotation" else "VEC3")
            samplers.append({"input": t, "output": v, "interpolation": "LINEAR"})
            chans.append({"sampler": len(samplers) - 1, "target": {"node": node, "path": path}})
        self.json.setdefault("animations", []).append({"name": name, "samplers": samplers, "channels": chans})

    # output
    def glb(self) -> bytes:
        for key in ("meshes", "materials"):
            if not self.json[key]:
                del self.json[key]
        while len(self.bin) % 4:
            self.bin.append(0)
        self.json["buffers"][0]["byteLength"] = len(self.bin)
        text = json.dumps(self.json, separators=(",", ":")).encode("utf-8")
        text += b" " * (-len(text) % 4)
        total = 12 + 8 + len(text) + 8 + len(self.bin)
        return (struct.pack("<4sII", b"glTF", 2, total)
                + struct.pack("<I4s", len(text), b"JSON") + text
                + struct.pack("<I4s", len(self.bin), b"BIN\x00") + bytes(self.bin))


def translation_matrix(t):
    """Column-major 4x4 translation."""
    return [1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1, 0, t[0], t[1], t[2], 1]


def bounds_of(*geos):
    los, his = zip(*(g.bounds() for g in geos))
    return tuple(map(min, zip(*los))), tuple(map(max, zip(*his)))


def collision_box(doc, parent, *geos):
    """A `col_box` empty filling the geometries' bounds: its translation is the box centre and
    its scale the half extents, as a Blender cube empty exports (CONVENTIONS.md). It has no
    mesh, so nothing extra is drawn."""
    lo, hi = bounds_of(*geos)
    return doc.node("col_box", parent, translation=scale(add(lo, hi), 0.5), scale=scale(sub(hi, lo), 0.5))
