#!/usr/bin/env python3
"""Generate the game's dice sprite set with only Python stdlib."""

from __future__ import annotations

import argparse
import math
import struct
import zlib
from pathlib import Path


SIZE = 128
SUPERSAMPLE = 4
FACE_SIZE = 62.0
FACE_RADIUS = 13.0
FACE_CENTER = (63.5, 58.0)
EDGE_STEPS = 8


Color = tuple[int, int, int, int]


def blend(dst: list[int], index: int, color: Color) -> None:
    sr, sg, sb, sa = color
    if sa <= 0:
        return
    dr, dg, db, da = dst[index : index + 4]
    src_a = sa / 255.0
    dst_a = da / 255.0
    out_a = src_a + dst_a * (1.0 - src_a)
    if out_a <= 0.0:
        dst[index : index + 4] = [0, 0, 0, 0]
        return
    dst[index] = round((sr * src_a + dr * dst_a * (1.0 - src_a)) / out_a)
    dst[index + 1] = round((sg * src_a + dg * dst_a * (1.0 - src_a)) / out_a)
    dst[index + 2] = round((sb * src_a + db * dst_a * (1.0 - src_a)) / out_a)
    dst[index + 3] = round(out_a * 255.0)


class Canvas:
    def __init__(self, size: int = SIZE, supersample: int = SUPERSAMPLE) -> None:
        self.size = size
        self.supersample = supersample
        self.width = size * supersample
        self.height = size * supersample
        self.pixels = [0] * (self.width * self.height * 4)

    def _blend_at(self, x: int, y: int, color: Color) -> None:
        if 0 <= x < self.width and 0 <= y < self.height:
            blend(self.pixels, (y * self.width + x) * 4, color)

    def ellipse(self, cx: float, cy: float, rx: float, ry: float, color: Color) -> None:
        x0 = max(0, math.floor((cx - rx) * self.supersample) - 1)
        x1 = min(self.width, math.ceil((cx + rx) * self.supersample) + 1)
        y0 = max(0, math.floor((cy - ry) * self.supersample) - 1)
        y1 = min(self.height, math.ceil((cy + ry) * self.supersample) + 1)
        for y in range(y0, y1):
            py = (y + 0.5) / self.supersample
            for x in range(x0, x1):
                px = (x + 0.5) / self.supersample
                if ((px - cx) / rx) ** 2 + ((py - cy) / ry) ** 2 <= 1.0:
                    self._blend_at(x, y, color)

    def rounded_rect(
        self,
        cx: float,
        cy: float,
        width: float,
        height: float,
        radius: float,
        rotation: float,
        color: Color,
    ) -> None:
        cos_a = math.cos(-rotation)
        sin_a = math.sin(-rotation)
        diagonal = math.hypot(width, height) * 0.5 + radius + 2.0
        x0 = max(0, math.floor((cx - diagonal) * self.supersample))
        x1 = min(self.width, math.ceil((cx + diagonal) * self.supersample))
        y0 = max(0, math.floor((cy - diagonal) * self.supersample))
        y1 = min(self.height, math.ceil((cy + diagonal) * self.supersample))
        half_w = width * 0.5
        half_h = height * 0.5
        inner_w = half_w - radius
        inner_h = half_h - radius
        for y in range(y0, y1):
            py = (y + 0.5) / self.supersample - cy
            for x in range(x0, x1):
                px = (x + 0.5) / self.supersample - cx
                local_x = px * cos_a - py * sin_a
                local_y = px * sin_a + py * cos_a
                qx = abs(local_x) - inner_w
                qy = abs(local_y) - inner_h
                outside = math.hypot(max(qx, 0.0), max(qy, 0.0))
                inside = min(max(qx, qy), 0.0)
                if outside + inside <= radius:
                    self._blend_at(x, y, color)

    def line(
        self,
        x0: float,
        y0: float,
        x1: float,
        y1: float,
        width: float,
        color: Color,
    ) -> None:
        min_x = max(0, math.floor((min(x0, x1) - width) * self.supersample))
        max_x = min(self.width, math.ceil((max(x0, x1) + width) * self.supersample))
        min_y = max(0, math.floor((min(y0, y1) - width) * self.supersample))
        max_y = min(self.height, math.ceil((max(y0, y1) + width) * self.supersample))
        dx = x1 - x0
        dy = y1 - y0
        length_sq = dx * dx + dy * dy
        for y in range(min_y, max_y):
            py = (y + 0.5) / self.supersample
            for x in range(min_x, max_x):
                px = (x + 0.5) / self.supersample
                t = 0.0
                if length_sq > 0.0:
                    t = max(0.0, min(1.0, ((px - x0) * dx + (py - y0) * dy) / length_sq))
                closest_x = x0 + dx * t
                closest_y = y0 + dy * t
                if math.hypot(px - closest_x, py - closest_y) <= width * 0.5:
                    self._blend_at(x, y, color)

    def downsample(self) -> bytes:
        scale = self.supersample
        output = bytearray(self.size * self.size * 4)
        for y in range(self.size):
            for x in range(self.size):
                totals = [0, 0, 0, 0]
                for sy in range(scale):
                    for sx in range(scale):
                        src = (((y * scale + sy) * self.width) + (x * scale + sx)) * 4
                        for channel in range(4):
                            totals[channel] += self.pixels[src + channel]
                dst = (y * self.size + x) * 4
                sample_count = scale * scale
                for channel in range(4):
                    output[dst + channel] = round(totals[channel] / sample_count)
        return bytes(output)


def transform_point(
    center: tuple[float, float], local_x: float, local_y: float, rotation: float
) -> tuple[float, float]:
    cos_a = math.cos(rotation)
    sin_a = math.sin(rotation)
    return (
        center[0] + local_x * cos_a - local_y * sin_a,
        center[1] + local_x * sin_a + local_y * cos_a,
    )


def pip_layout(face: int) -> list[tuple[float, float]]:
    d = 16.5
    return {
        1: [(0.0, 0.0)],
        2: [(-d, -d), (d, d)],
        3: [(-d, -d), (0.0, 0.0), (d, d)],
        4: [(-d, -d), (d, -d), (-d, d), (d, d)],
        5: [(-d, -d), (d, -d), (0.0, 0.0), (-d, d), (d, d)],
        6: [(-d, -d), (d, -d), (-d, 0.0), (d, 0.0), (-d, d), (d, d)],
    }[face]


def draw_pips(canvas: Canvas, face: int, center: tuple[float, float], rotation: float) -> None:
    shadow_color = (0, 0, 0, 46)
    pip_color = (31, 38, 48, 238)
    shine_color = (255, 255, 255, 42)
    for local_x, local_y in pip_layout(face):
        x, y = transform_point(center, local_x, local_y, rotation)
        canvas.ellipse(x + 0.7, y + 1.1, 5.4, 5.4, shadow_color)
        canvas.ellipse(x, y, 5.0, 5.0, pip_color)
        canvas.ellipse(x - 1.3, y - 1.6, 1.3, 1.1, shine_color)


def draw_die(canvas: Canvas, face: int, rotation_degrees: float = 0.0) -> None:
    rotation = math.radians(rotation_degrees)
    cx, cy = FACE_CENTER
    canvas.ellipse(cx + 5.0, cy + 42.0, 34.0, 10.5, (0, 0, 0, 34))

    for step in range(EDGE_STEPS, 0, -1):
        t = step / EDGE_STEPS
        edge = round(154 + 34 * (1.0 - t))
        canvas.rounded_rect(
            cx + step * 0.8,
            cy + step * 0.95,
            FACE_SIZE,
            FACE_SIZE,
            FACE_RADIUS,
            rotation,
            (edge, edge + 3, edge + 8, 242),
        )

    canvas.rounded_rect(
        cx + 1.2,
        cy + 1.8,
        FACE_SIZE + 3.8,
        FACE_SIZE + 3.8,
        FACE_RADIUS + 1.5,
        rotation,
        (94, 104, 119, 182),
    )
    canvas.rounded_rect(
        cx,
        cy,
        FACE_SIZE,
        FACE_SIZE,
        FACE_RADIUS,
        rotation,
        (245, 248, 252, 255),
    )
    canvas.rounded_rect(
        cx - 3.2,
        cy - 4.0,
        FACE_SIZE * 0.78,
        FACE_SIZE * 0.34,
        FACE_RADIUS * 0.68,
        rotation,
        (255, 255, 255, 58),
    )

    top_left = transform_point((cx, cy), -FACE_SIZE * 0.30, -FACE_SIZE * 0.45, rotation)
    top_right = transform_point((cx, cy), FACE_SIZE * 0.25, -FACE_SIZE * 0.48, rotation)
    canvas.line(top_left[0], top_left[1], top_right[0], top_right[1], 1.5, (255, 255, 255, 105))
    draw_pips(canvas, face, (cx, cy), rotation)


def png_chunk(kind: bytes, payload: bytes) -> bytes:
    return (
        struct.pack(">I", len(payload))
        + kind
        + payload
        + struct.pack(">I", zlib.crc32(kind + payload) & 0xFFFFFFFF)
    )


def write_png(path: Path, width: int, height: int, rgba: bytes) -> None:
    rows = bytearray()
    stride = width * 4
    for y in range(height):
        rows.append(0)
        rows.extend(rgba[y * stride : (y + 1) * stride])
    payload = zlib.compress(bytes(rows), level=9)
    path.write_bytes(
        b"\x89PNG\r\n\x1a\n"
        + png_chunk(b"IHDR", struct.pack(">IIBBBBB", width, height, 8, 6, 0, 0, 0))
        + png_chunk(b"IDAT", payload)
        + png_chunk(b"IEND", b"")
    )


def render_sprite(face: int, rotation_degrees: float) -> bytes:
    canvas = Canvas()
    draw_die(canvas, face, rotation_degrees)
    return canvas.downsample()


def make_preview(sprites: list[tuple[str, bytes]], output: Path) -> None:
    cell = SIZE
    cols = 6
    rows = math.ceil(len(sprites) / cols)
    sheet = bytearray([0] * (cols * cell * rows * cell * 4))
    sheet_width = cols * cell
    for index, (_, rgba) in enumerate(sprites):
        col = index % cols
        row = index // cols
        for y in range(cell):
            for x in range(cell):
                src = (y * cell + x) * 4
                dst = ((row * cell + y) * sheet_width + col * cell + x) * 4
                sheet[dst : dst + 4] = rgba[src : src + 4]
    write_png(output, sheet_width, rows * cell, bytes(sheet))


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--out", type=Path, default=Path("assets/ui/dice"))
    parser.add_argument("--preview", type=Path)
    args = parser.parse_args()

    args.out.mkdir(parents=True, exist_ok=True)
    generated: list[tuple[str, bytes]] = []

    for face in range(1, 7):
        name = f"die_{face}.png"
        rgba = render_sprite(face, 0.0)
        write_png(args.out / name, SIZE, SIZE, rgba)
        generated.append((name, rgba))

    roll_faces = [1, 5, 2, 6, 3, 4, 1, 6, 2, 5, 3, 1, 4, 6, 2, 5]
    roll_angles = [-14, -7, 8, 18, 11, -10, -20, -4, 13, 22, 7, -12, -18, 2, 16, 9]
    for frame, (face, angle) in enumerate(zip(roll_faces, roll_angles)):
        name = f"roll_{frame:02}.png"
        rgba = render_sprite(face, angle)
        write_png(args.out / name, SIZE, SIZE, rgba)
        generated.append((name, rgba))

    if args.preview:
        make_preview(generated, args.preview)


if __name__ == "__main__":
    main()
