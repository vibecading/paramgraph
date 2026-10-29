"""Shaded axonometric preview of a `pg obj` / `pg diff-obj` export, no CAD needed.

usage: python -I tools/preview.py model.obj out.png [--az 35] [--el 30] [--width 1400]

Groups are coloured: added green, removed red, moved amber, unchanged grey;
grade groups (measured, rule, assumed, published) get their own colours.
A depth buffer decides which face is in front at every pixel.
"""
import math
import sys

import cv2
import numpy as np

COL = {'added': (70, 170, 40), 'removed': (50, 50, 220), 'moved': (20, 170, 240), 'unchanged': (185, 185, 185),
       'measured': (138, 33, 224), 'rule': (255, 140, 79), 'assumed': (5, 183, 242), 'published': (79, 125, 47)}


def load(path):
    V, faces, group = [], [], 'unchanged'
    for line in open(path):
        t = line.split()
        if not t:
            continue
        if t[0] == 'v':
            V.append([float(x) for x in t[1:4]])
        elif t[0] == 'g':
            group = t[1]
        elif t[0] == 'f':
            faces.append(([int(x) - 1 for x in t[1:]], group))
    return np.array(V), faces


def main():
    src, out = sys.argv[1], sys.argv[2]
    opt = lambda k, d: float(sys.argv[sys.argv.index(k) + 1]) if k in sys.argv else d
    az, el, W = math.radians(opt('--az', 35)), math.radians(opt('--el', 30)), int(opt('--width', 1400))
    V, faces = load(src)
    # view: rotate about z by az, then tilt by el; looking from the south-west and above
    Rz = np.array([[math.cos(az), math.sin(az), 0], [-math.sin(az), math.cos(az), 0], [0, 0, 1]])
    Rx = np.array([[1, 0, 0], [0, math.sin(el), math.cos(el)], [0, -math.cos(el), math.sin(el)]])
    P = V @ (Rx @ Rz).T  # x right, y up, z towards the viewer
    lo, hi = P[:, :2].min(0), P[:, :2].max(0)
    s = (W - 80) / max(hi[0] - lo[0], 1e-9)
    H = int((hi[1] - lo[1]) * s + 80)
    img = np.full((H, W, 3), 255, np.uint8)
    zbuf = np.full((H, W), -np.inf)
    light = np.array([0.4, 0.5, 0.77])
    yy, xx = np.mgrid[0:H, 0:W]
    edges = []
    for idx, g in faces:
        q = P[idx]
        n = np.cross(q[1] - q[0], q[2] - q[0])
        if np.linalg.norm(n) < 1e-12 or abs(n[2]) < 1e-9 * np.linalg.norm(n):
            continue  # degenerate, or seen exactly edge-on
        n /= np.linalg.norm(n)
        shade = 0.55 + 0.45 * abs(n @ light)
        base = np.array(COL.get(g, (150, 150, 150)), float)
        pts = np.stack([(q[:, 0] - lo[0]) * s + 40, (hi[1] - q[:, 1]) * s + 40], 1)
        mask = np.zeros((H, W), np.uint8)
        cv2.fillPoly(mask, [pts.astype(np.int32)], 1)
        m = mask.astype(bool)
        # depth of the face's plane at every covered pixel (z towards the viewer)
        X = (xx[m] - 40) / s + lo[0]
        Y = hi[1] - (yy[m] - 40) / s
        Z = q[0, 2] - (n[0] * (X - q[0, 0]) + n[1] * (Y - q[0, 1])) / n[2]
        front = Z > zbuf[m] + 1e-6
        sel = (yy[m][front], xx[m][front])
        zbuf[sel] = Z[front]
        img[sel] = (base * shade).astype(np.uint8)
        edges.append((pts, q))
    # outlines where the edge is the nearest surface
    for pts, q in edges:
        for k in range(len(pts)):
            a, b = pts[k], pts[(k + 1) % len(pts)]
            za, zb = q[k, 2], q[(k + 1) % len(q), 2]
            for t in np.linspace(0, 1, max(2, int(np.hypot(*(b - a))))):
                x, y = (a + t * (b - a)).round().astype(int)
                if 0 <= x < W and 0 <= y < H and za + t * (zb - za) >= zbuf[y, x] - 0.05:
                    img[y, x] = (60, 60, 60)
    y = 24
    for g in sorted({g for _, g in faces}):
        cv2.rectangle(img, (20, y - 12), (36, y + 2), COL.get(g, (150, 150, 150)), -1)
        cv2.putText(img, g, (44, y), cv2.FONT_HERSHEY_SIMPLEX, 0.55, (40, 40, 40), 1, cv2.LINE_AA)
        y += 22
    cv2.imwrite(out, img)
    print(out, img.shape[1], 'x', img.shape[0])


if __name__ == '__main__':
    main()
