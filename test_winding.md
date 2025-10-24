# Cube Face Winding Analysis

## +Y Face (TOP) - Normal: [0, 1, 0] pointing UP
Viewing from ABOVE (looking down at the face):
```
Vertices in order:
8: [-0.5, 0.5, -0.5]  = back-left
9: [0.5, 0.5, -0.5]   = back-right
10: [0.5, 0.5, 0.5]   = front-right
11: [-0.5, 0.5, 0.5]  = front-left

Visual layout (Z goes up, X goes right):
    11--------10
    |          |
    |    +Y    |
    |          |
    8---------9

Order: 8→9→10→11 goes: back-left → back-right → front-right → front-left
This is CLOCKWISE when viewed from above! ❌

Should be: 8→11→10→9 for CCW
Or reorder vertices: 8,9,10,11 → 8,11,10,9
```

## -Y Face (BOTTOM) - Normal: [0, -1, 0] pointing DOWN
Viewing from BELOW (looking up at the face):
```
Vertices in order:
12: [-0.5, -0.5, 0.5]   = front-left
13: [0.5, -0.5, 0.5]    = front-right
14: [0.5, -0.5, -0.5]   = back-right
15: [-0.5, -0.5, -0.5]  = back-left

Visual layout (Z goes up when viewed from below, X goes right):
    15--------14
    |          |
    |    -Y    |
    |          |
    12--------13

Order: 12→13→14→15 goes: front-left → front-right → back-right → back-left
This is CLOCKWISE when viewed from below! ❌

Should be: 12→15→14→13 for CCW
```

## +Z Face (FRONT) - Normal: [0, 0, 1] pointing FORWARD
Viewing from FRONT (looking at the face head-on from +Z):
```
Vertices in order:
16: [-0.5, -0.5, 0.5]  = bottom-left
17: [0.5, -0.5, 0.5]   = bottom-right
18: [0.5, 0.5, 0.5]    = top-right
19: [-0.5, 0.5, 0.5]   = top-left

Visual layout (Y goes up, X goes right):
    19--------18
    |          |
    |    +Z    |
    |          |
    16--------17

Order: 16→17→18→19 goes: BL → BR → TR → TL
This is CLOCKWISE! ❌

Should be: 16→19→18→17 for CCW
```

## -Z Face (BACK) - Normal: [0, 0, -1] pointing BACKWARD
Viewing from BACK (from -Z looking toward origin):
```
Vertices in order:
20: [0.5, -0.5, -0.5]   = bottom-left (when viewed from back, +X is on left)
21: [-0.5, -0.5, -0.5]  = bottom-right
22: [-0.5, 0.5, -0.5]   = top-right
23: [0.5, 0.5, -0.5]    = top-left

Visual layout (Y goes up, X goes LEFT):
    23--------22
    |          |
    |    -Z    |
    |          |
    20--------21

Order: 20→21→22→23 goes: BL → BR → TR → TL
This is CLOCKWISE! ❌

Should be: 20→23→22→21 for CCW
```

## CONCLUSION
The vertices are ordered CLOCKWISE for most faces!
We need to either:
1. Reorder the vertices in the vertex array, OR
2. Change the indices to reverse the winding

Easier solution: Fix the indices to reverse the winding.
