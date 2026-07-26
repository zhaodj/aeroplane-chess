#!/usr/bin/env python3
"""Pack the generated PNG favicon sizes into a browser-compatible ICO file."""

from __future__ import annotations

import struct
import sys
from pathlib import Path


def make_ico(output: Path, png_paths: list[Path]) -> None:
    png_data = [path.read_bytes() for path in png_paths]
    header = struct.pack("<HHH", 0, 1, len(png_data))
    entries = []
    offset = 6 + 16 * len(png_data)

    for path, data in zip(png_paths, png_data):
        size = int(path.stem.rsplit("-", 1)[-1].split("x", 1)[0])
        dimension = 0 if size >= 256 else size
        entries.append(
            struct.pack(
                "<BBBBHHII",
                dimension,
                dimension,
                0,
                0,
                1,
                32,
                len(data),
                offset,
            )
        )
        offset += len(data)

    output.write_bytes(header + b"".join(entries) + b"".join(png_data))


def main() -> None:
    if len(sys.argv) != 2:
        raise SystemExit("usage: generate-favicon-ico.py OUTPUT")

    output = Path(sys.argv[1])
    png_paths = [
        output.with_name("favicon-16x16.png"),
        output.with_name("favicon-32x32.png"),
        output.with_name("favicon-48x48.png"),
        output.with_name("favicon-64x64.png"),
    ]
    missing = [path for path in png_paths if not path.is_file()]
    if missing:
        raise SystemExit(f"missing PNG favicon input: {missing[0]}")
    make_ico(output, png_paths)


if __name__ == "__main__":
    main()
