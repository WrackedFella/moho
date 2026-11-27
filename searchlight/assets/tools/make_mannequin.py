#!/usr/bin/env python3
"""The shared humanoid placeholder: a segmented mannequin on the engine's 16 humanoid bones
(Unreal mannequin names), one box per bone, rigidly skinned.

Standard library only; ported from searchlight-assets (source/generators/mannequin.py at
d94a66c), whose exports it reproduces byte for byte. It is the game's no-art fallback: an art
folder's files at the same relative paths take its place one file at a time (ADR-0042).

    python3 -I make_mannequin.py [output-directory]

The output directory defaults to characters/mannequin/ beside tools/. Outputs:
- mannequin.glb     enemies and any third-person body: every box, head included
- mannequin_fp.glb  the player's body: same rig and clips, no head or neck mesh, so the
                    camera never sits inside one (the head bone stays for the rig)
- head.glb          head and neck as one rigid part that rides the `head` bone, for the
                    player's shadow

1.8 m tall, +Y up, facing +Z (the character's left is +X), origin on the floor between the feet.
Bones have identity rest rotations, so a clip's rotation is the bone's bend from standing.
"""

from __future__ import annotations

import math
import sys
from pathlib import Path

# `python3 -I` leaves the script's own folder off the path; no bytecode is left beside the tools.
sys.dont_write_bytecode = True
sys.path.insert(0, str(Path(__file__).resolve().parent))

from gltf_writer import (IDENTITY, Document, Geometry, add, dot, euler_x, euler_y, euler_z, length,
                         normalize, quat_between, quat_conj, quat_mul, quat_rotate, scale, sub,
                         translation_matrix)

# name, parent, offset from parent joint (metres)
BONES = (
    ("pelvis", None, (0.0, 0.95, 0.0)),
    ("spine_01", "pelvis", (0.0, 0.15, 0.0)),
    ("neck_01", "spine_01", (0.0, 0.40, 0.0)),
    ("head", "neck_01", (0.0, 0.10, 0.0)),
    ("upperarm_l", "spine_01", (0.21, 0.36, 0.0)),
    ("lowerarm_l", "upperarm_l", (0.0, -0.30, 0.0)),
    ("hand_l", "lowerarm_l", (0.0, -0.27, 0.0)),
    ("upperarm_r", "spine_01", (-0.21, 0.36, 0.0)),
    ("lowerarm_r", "upperarm_r", (0.0, -0.30, 0.0)),
    ("hand_r", "lowerarm_r", (0.0, -0.27, 0.0)),
    ("thigh_l", "pelvis", (0.10, -0.05, 0.0)),
    ("calf_l", "thigh_l", (0.0, -0.42, 0.0)),
    ("foot_l", "calf_l", (0.0, -0.40, 0.0)),
    ("thigh_r", "pelvis", (-0.10, -0.05, 0.0)),
    ("calf_r", "thigh_r", (0.0, -0.42, 0.0)),
    ("foot_r", "calf_r", (0.0, -0.40, 0.0)),
)
INDEX = {name: i for i, (name, _, _) in enumerate(BONES)}
PARENT = {name: parent for name, parent, _ in BONES}
OFFSET = {name: offset for name, _, offset in BONES}

UPPER_ARM, LOWER_ARM = 0.30, 0.27

# Each box in its bone's space: centre, size, material.
BOXES = {
    "pelvis": ((0.0, 0.02, 0.0), (0.32, 0.18, 0.20), "cloth"),
    "spine_01": ((0.0, 0.18, 0.0), (0.38, 0.44, 0.22), "cloth"),
    "neck_01": ((0.0, 0.05, 0.0), (0.10, 0.10, 0.10), "skin"),
    "head": ((0.0, 0.12, 0.01), (0.20, 0.24, 0.22), "skin"),
    "upperarm_l": ((0.0, -0.15, 0.0), (0.10, 0.30, 0.10), "cloth"),
    "lowerarm_l": ((0.0, -0.135, 0.0), (0.09, 0.27, 0.09), "cloth"),
    "hand_l": ((0.0, -0.06, 0.0), (0.05, 0.12, 0.08), "skin"),
    "thigh_l": ((0.0, -0.21, 0.0), (0.14, 0.42, 0.14), "trousers"),
    "calf_l": ((0.0, -0.20, 0.0), (0.12, 0.40, 0.12), "trousers"),
    "foot_l": ((0.0, -0.04, 0.05), (0.10, 0.08, 0.24), "boots"),
}
for _bone in ("upperarm", "lowerarm", "hand", "thigh", "calf", "foot"):
    BOXES[f"{_bone}_r"] = BOXES[f"{_bone}_l"]

COLOURS = {
    "cloth": (0.42, 0.45, 0.47), "trousers": (0.30, 0.32, 0.33),
    "boots": (0.12, 0.11, 0.10), "skin": (0.72, 0.60, 0.50),
}
HEAD_PARTS = ("head", "neck_01")


def rest_global(name):
    pos = (0.0, 0.0, 0.0)
    while name:
        pos = add(pos, OFFSET[name])
        name = PARENT[name]
    return pos


# --- poses: dict bone -> local rotation, plus "root" -> pelvis translation offset ---

def legs_walk(phase, swing, knee, lean=0.0):
    """Opposed leg swing; phase in [0, 1)."""
    s = math.sin(2 * math.pi * phase)
    lift_l = max(0.0, math.sin(2 * math.pi * phase + math.pi / 2))
    lift_r = max(0.0, math.sin(2 * math.pi * phase - math.pi / 2))
    return {
        "thigh_l": euler_x(-swing * s), "calf_l": euler_x(knee * lift_l),
        "thigh_r": euler_x(swing * s), "calf_r": euler_x(knee * lift_r),
        "spine_01": euler_x(lean),
    }


def arms_swing(phase, swing):
    s = math.sin(2 * math.pi * phase)
    return {
        "upperarm_l": euler_x(swing * s), "lowerarm_l": euler_x(-15.0),
        "upperarm_r": euler_x(-swing * s), "lowerarm_r": euler_x(-15.0),
    }


def crouch(depth=1.0):
    """Knees forward, hips back and down: thighs -80, calves +90, feet back to level."""
    t, c = -80.0 * depth, 90.0 * depth
    drop = 0.353 * depth
    return {
        "root": (0.0, -drop, -0.25 * depth),
        "thigh_l": euler_x(t), "calf_l": euler_x(c), "foot_l": euler_x(-(t + c)),
        "thigh_r": euler_x(t), "calf_r": euler_x(c), "foot_r": euler_x(-(t + c)),
        "spine_01": euler_x(18.0 * depth),
    }


def two_bone(shoulder, target, pole):
    """Elbow position for an arm from shoulder to target, bending towards pole."""
    d_vec = sub(target, shoulder)
    d = min(length(d_vec), UPPER_ARM + LOWER_ARM - 1e-3)
    u = normalize(d_vec)
    x = (UPPER_ARM ** 2 - LOWER_ARM ** 2 + d * d) / (2 * d)
    h = math.sqrt(max(0.0, UPPER_ARM ** 2 - x * x))
    p = sub(pole, scale(u, dot(pole, u)))
    return add(add(shoulder, scale(u, x)), scale(normalize(p), h))


def arm_to(side, hand_target, pole, spine_rot=IDENTITY, root=(0.0, 0.0, 0.0), pelvis_rot=IDENTITY):
    """Local rotations for upperarm and lowerarm that put the wrist at hand_target (model space)."""
    parent_rot = quat_mul(pelvis_rot, spine_rot)
    pelvis = add(OFFSET["pelvis"], root)
    spine = add(pelvis, quat_rotate(pelvis_rot, OFFSET["spine_01"]))
    shoulder = add(spine, quat_rotate(parent_rot, OFFSET[f"upperarm_{side}"]))
    elbow = two_bone(shoulder, hand_target, pole)
    down = quat_rotate(parent_rot, (0.0, -1.0, 0.0))
    upper_world = quat_mul(quat_between(down, sub(elbow, shoulder)), parent_rot)
    lower_down = quat_rotate(upper_world, (0.0, -1.0, 0.0))
    lower_world = quat_mul(quat_between(lower_down, sub(hand_target, elbow)), upper_world)
    return {
        f"upperarm_{side}": quat_mul(quat_conj(parent_rot), upper_world),
        f"lowerarm_{side}": quat_mul(quat_conj(upper_world), lower_world),
        # The hand keeps the model's orientation, so a rifle attached at hand_r points +Z.
        f"hand_{side}": quat_conj(lower_world),
    }


# Wrist positions holding the rifle at the ready, model space when standing. GRIP_L is the
# rifle's grip_l point when its grip_r sits at GRIP_R (see rifle.py).
GRIP_R = (-0.10, 1.28, 0.22)
GRIP_L = (-0.10, 1.31, 0.44)


def hold_rifle(spine_deg=0.0, root=(0.0, 0.0, 0.0)):
    spine_rot = euler_x(spine_deg)
    pose = {"spine_01": spine_rot}
    grip_r, grip_l = add(GRIP_R, root), add(GRIP_L, root)
    pose.update(arm_to("r", grip_r, (-1.0, -1.0, 0.0), spine_rot, root))
    pose.update(arm_to("l", grip_l, (1.0, -1.0, -0.3), spine_rot, root))
    return pose


def prone(arm_phase=None, leg_phase=None):
    pose = {"root": (0.0, -0.84, -0.55), "pelvis": euler_x(90.0),
            "thigh_l": euler_x(-5.0), "thigh_r": euler_x(-5.0),
            "foot_l": euler_x(60.0), "foot_r": euler_x(60.0),
            "spine_01": euler_x(-10.0),
            "upperarm_l": euler_x(-175.0), "upperarm_r": euler_x(-175.0),
            "neck_01": euler_x(-50.0)}
    if arm_phase is not None:
        s = math.sin(2 * math.pi * arm_phase)
        pose["upperarm_l"] = euler_x(-175.0 + 25.0 * s)
        pose["upperarm_r"] = euler_x(-175.0 - 25.0 * s)
        pose["thigh_l"] = quat_mul(euler_z(12.0), euler_x(-5.0 - 20.0 * max(0.0, -s)))
        pose["calf_l"] = euler_x(60.0 * max(0.0, -s))
        pose["thigh_r"] = quat_mul(euler_z(-12.0), euler_x(-5.0 - 20.0 * max(0.0, s)))
        pose["calf_r"] = euler_x(60.0 * max(0.0, s))
        pose["pelvis"] = quat_mul(euler_x(90.0), euler_y(6.0 * s))
    return pose


def merge(*poses):
    out = {}
    for p in poses:
        out.update(p)
    return out


def death(t):
    """Knees give way, then fall backwards onto the back."""
    if t <= 0.4:
        k = t / 0.4
        p = crouch(0.6 * k)
        p["spine_01"] = euler_x(-15.0 * k)
        p["upperarm_l"] = euler_z(25.0 * k)
        p["upperarm_r"] = euler_z(-25.0 * k)
        return on_floor(p)
    k = min(1.0, (t - 0.4) / 0.6)
    ease = 1 - (1 - k) ** 2
    p = crouch(0.6 * (1 - ease))
    # From the half-crouch (pelvis 0.21 m down, 0.15 m back) to lying on the back, pelvis 0.11 m up.
    p["root"] = (0.0, -0.212 * (1 - ease) - 0.84 * ease, -0.15 * (1 - ease) - 0.55 * ease)
    p["pelvis"] = euler_x(-90.0 * ease)
    p["spine_01"] = euler_x(-15.0 * (1 - ease))
    p["upperarm_l"] = euler_z(25.0 + 35.0 * ease)
    p["upperarm_r"] = euler_z(-25.0 - 35.0 * ease)
    p["thigh_l"] = euler_x(-48.0 * (1 - ease) - 20.0 * ease)
    p["calf_l"] = euler_x(54.0 * (1 - ease) + 30.0 * ease)
    p["thigh_r"] = euler_x(-48.0 * (1 - ease))
    p["calf_r"] = euler_x(54.0 * (1 - ease))
    p["foot_l"] = p["foot_r"] = IDENTITY
    return on_floor(p)


# name, duration (s), keys, pose(t in [0, 1])
CLIPS = (
    ("idle", 2.0, 5, lambda t: {"spine_01": euler_x(1.5 * math.sin(2 * math.pi * t))}),
    ("walk", 1.0, 9, lambda t: merge(legs_walk(t, 25.0, 40.0), arms_swing(t, 15.0))),
    ("run", 0.7, 9, lambda t: merge(legs_walk(t, 45.0, 80.0, lean=10.0), arms_swing(t, 35.0),
                                    {"lowerarm_l": euler_x(-80.0), "lowerarm_r": euler_x(-80.0)})),
    ("crouch_idle", 2.0, 3, lambda t: crouch()),
    ("crouch_walk", 1.2, 9, lambda t: merge(
        crouch(),
        {"thigh_l": euler_x(-80.0 - 15.0 * math.sin(2 * math.pi * t)),
         "thigh_r": euler_x(-80.0 + 15.0 * math.sin(2 * math.pi * t))})),
    ("prone_idle", 2.0, 3, lambda t: prone()),
    ("crawl", 1.6, 9, lambda t: prone(arm_phase=t)),
    ("rifle_idle", 2.0, 5, lambda t: hold_rifle(1.0 * math.sin(2 * math.pi * t))),
    ("rifle_walk", 1.0, 9, lambda t: merge(legs_walk(t, 25.0, 40.0), hold_rifle())),
    ("death", 1.2, 13, death),
)
AIRBORNE_OK = {"run"}   # a run has both feet off the ground between strides
LOOPING = {"idle", "walk", "run", "crouch_idle", "crouch_walk", "prone_idle", "crawl", "rifle_idle", "rifle_walk"}


def bone_geometry(skip=()):
    """One primitive per material, each box skinned to its bone, in bind (model) space."""
    parts = {}
    for name, (centre, size, material) in BOXES.items():
        if name in skip:
            continue
        geo = parts.setdefault(material, Geometry())
        geo.box(add(rest_global(name), centre), size, joint=INDEX[name])
    return parts


def character(scene_name, skip=()):
    doc = Document(scene_name)
    root = doc.node(scene_name)
    joints = {}
    for name, parent, offset in BONES:
        joints[name] = doc.node(name, joints[parent] if parent else root, translation=offset)
    materials = {m: doc.material(m, COLOURS[m]) for m in COLOURS}
    parts = bone_geometry(skip)
    mesh = doc.mesh(f"{scene_name}_body", [(geo, materials[m]) for m, geo in sorted(parts.items())], skinned=True)
    skin = doc.skin(scene_name, [joints[n] for n, _, _ in BONES],
                    [translation_matrix(scale(rest_global(n), -1.0)) for n, _, _ in BONES], joints["pelvis"])
    # A skinned mesh node sits at the scene root: glTF ignores its parents' transforms anyway.
    doc.json["nodes"].append({"name": f"{scene_name}_body", "mesh": mesh, "skin": skin})
    doc.json["scenes"][0]["nodes"].append(len(doc.json["nodes"]) - 1)
    for clip, duration, keys, fn in CLIPS:
        add_clip(doc, joints, clip, duration, keys, fn)
    return doc.glb()


def forward(pose):
    """Model-space (position, rotation) of every bone for a pose."""
    world = {}
    for name, parent, offset in BONES:
        local = pose.get(name, IDENTITY)
        if parent is None:
            world[name] = (add(offset, pose.get("root", (0.0, 0.0, 0.0))), local)
        else:
            ppos, prot = world[parent]
            world[name] = (add(ppos, quat_rotate(prot, offset)), quat_mul(prot, local))
    return world


def lowest_point(pose):
    """The lowest box corner in model space: below 0 means the body is in the floor."""
    world, low = forward(pose), float("inf")
    for name, (centre, size, _) in BOXES.items():
        pos, rot = world[name]
        for sx in (-0.5, 0.5):
            for sy in (-0.5, 0.5):
                for sz in (-0.5, 0.5):
                    corner = add(centre, (size[0] * sx, size[1] * sy, size[2] * sz))
                    low = min(low, add(pos, quat_rotate(rot, corner))[1])
    return low


def on_floor(pose):
    """Raises the pose so its lowest point rests on the floor (for falls, which have no planted foot)."""
    root = pose.get("root", (0.0, 0.0, 0.0))
    pose["root"] = (root[0], root[1] - lowest_point(pose), root[2])
    return pose


def add_clip(doc, joints, name, duration, keys, fn):
    times = [duration * i / (keys - 1) for i in range(keys)]
    looping = name in LOOPING
    poses = [fn(0.0 if (looping and i == keys - 1) else i / (keys - 1)) for i in range(keys)]
    # Each key rests on the floor, which gives walks their hip bob; a run may leave it, never sink.
    for pose in poses:
        if name not in AIRBORNE_OK or lowest_point(pose) < 0.0:
            on_floor(pose)
    animated = sorted({b for p in poses for b in p if b != "root"}, key=lambda b: INDEX[b])
    channels = []
    for bone in animated:
        values, prev = [], None
        for p in poses:
            q = p.get(bone, IDENTITY)
            if prev is not None and dot(q, prev) < 0:   # keep slerp on the short arc
                q = tuple(-x for x in q)
            values.append(q)
            prev = q
        channels.append((joints[bone], "rotation", times, values))
    if any("root" in p for p in poses):
        values = [add(OFFSET["pelvis"], p.get("root", (0.0, 0.0, 0.0))) for p in poses]
        channels.append((joints["pelvis"], "translation", times, values))
    doc.animation(name, channels)


def head():
    doc = Document("head")
    root = doc.node("head_part")
    doc.node("attach", root)
    skin_mat = doc.material("skin", COLOURS["skin"])
    geo = Geometry()
    centre, size, _ = BOXES["head"]
    geo.box(centre, size)
    n_centre, n_size, _ = BOXES["neck_01"]
    geo.box(add(n_centre, scale(OFFSET["head"], -1.0)), n_size)
    doc.node("head_mesh", root, mesh=doc.mesh("head", [(geo, skin_mat)]))
    return doc.glb()


def build():
    return {
        "mannequin.glb": character("mannequin"),
        "mannequin_fp.glb": character("mannequin_fp", skip=HEAD_PARTS),
        "head.glb": head(),
    }


def main():
    default = Path(__file__).resolve().parent.parent / "characters" / "mannequin"
    target = Path(sys.argv[1]) if len(sys.argv) > 1 else default
    target.mkdir(parents=True, exist_ok=True)
    for name, data in build().items():
        (target / name).write_bytes(data)


if __name__ == "__main__":
    main()
