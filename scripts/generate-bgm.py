#!/usr/bin/env python3
"""Generate the soft looping background music asset."""

from __future__ import annotations

import argparse
import math
import shutil
import struct
import subprocess
import tempfile
import wave
from pathlib import Path


SAMPLE_RATE = 44_100
DEFAULT_DURATION = 64.0
SEGMENT_SECONDS = 8.0
CROSSFADE_SECONDS = 2.0
MASTER_GAIN = 2.5
TAU = math.tau

NOTE_FREQS = {
    "G2": 98.00,
    "A2": 110.00,
    "C3": 130.81,
    "D3": 146.83,
    "E3": 164.81,
    "F3": 174.61,
    "G3": 196.00,
    "A3": 220.00,
    "B3": 246.94,
    "C4": 261.63,
    "D4": 293.66,
    "E4": 329.63,
    "F4": 349.23,
    "G4": 392.00,
    "A4": 440.00,
    "B4": 493.88,
    "C5": 523.25,
    "D5": 587.33,
    "E5": 659.25,
    "G5": 783.99,
}

CHORDS = [
    ("Cmaj7", ["C3", "G3", "B3", "E4", "G4"]),
    ("Gsus", ["G2", "D3", "G3", "C4", "D4"]),
    ("Am7", ["A2", "E3", "G3", "C4", "E4"]),
    ("Fmaj7", ["F3", "C4", "E4", "A4", "C5"]),
    ("Cadd9", ["C3", "G3", "D4", "E4", "G4"]),
    ("Em7", ["E3", "B3", "D4", "G4", "B4"]),
    ("F6", ["F3", "C4", "D4", "A4", "C5"]),
    ("Gsus", ["G2", "D3", "G3", "C4", "D4"]),
]

BELL_PATTERN = ["G4", "C5", "E5", "D5", "G4", "B4", "D5", "G5"]


def project_root() -> Path:
    return Path(__file__).resolve().parents[1]


def quantize_loop_frequency(freq: float, duration: float) -> float:
    return round(freq * duration) / duration


def smoothstep(value: float) -> float:
    value = max(0.0, min(1.0, value))
    return value * value * (3.0 - 2.0 * value)


def note_phase(note: str) -> float:
    return (sum(ord(char) for char in note) % 37) / 37.0 * TAU


def note_pan(note: str) -> float:
    return ((sum(ord(char) for char in note) % 9) - 4) / 10.0


def chord_blend(time_seconds: float) -> tuple[list[str], float, list[str], float]:
    chord_count = len(CHORDS)
    position = (time_seconds % (SEGMENT_SECONDS * chord_count)) / SEGMENT_SECONDS
    current_index = int(position) % chord_count
    local = (position - current_index) * SEGMENT_SECONDS
    next_index = (current_index + 1) % chord_count

    blend = 0.0
    if local >= SEGMENT_SECONDS - CROSSFADE_SECONDS:
        blend = smoothstep((local - (SEGMENT_SECONDS - CROSSFADE_SECONDS)) / CROSSFADE_SECONDS)

    return CHORDS[current_index][1], 1.0 - blend, CHORDS[next_index][1], blend


def oscillator(freq: float, time_seconds: float, phase: float = 0.0) -> float:
    return math.sin(TAU * freq * time_seconds + phase)


def add_note(
    left: float,
    right: float,
    note: str,
    time_seconds: float,
    amplitude: float,
    duration: float,
) -> tuple[float, float]:
    base_freq = quantize_loop_frequency(NOTE_FREQS[note], duration)
    phase = note_phase(note)
    pan = note_pan(note)
    lfo = 0.82 + 0.18 * oscillator(2.0 / duration, time_seconds, phase * 0.5)
    tone = oscillator(base_freq, time_seconds, phase)
    tone += 0.14 * oscillator(base_freq * 2.0, time_seconds, phase * 1.73)
    value = tone * amplitude * lfo
    left += value * (1.0 - pan)
    right += value * (1.0 + pan)
    return left, right


def bell_events(duration: float) -> list[tuple[float, str, float]]:
    events: list[tuple[float, str, float]] = []
    event_time = 1.0
    index = 0
    while event_time <= duration - 4.5:
        note = BELL_PATTERN[index % len(BELL_PATTERN)]
        pan = -0.22 if index % 2 == 0 else 0.22
        events.append((event_time, note, pan))
        index += 1
        event_time += 2.0
    return events


def bell_sample(
    time_seconds: float,
    events: list[tuple[float, str, float]],
    duration: float,
) -> tuple[float, float]:
    left = 0.0
    right = 0.0
    start_index = max(0, int((time_seconds - 1.2) / 2.0) - 1)
    end_index = min(len(events), start_index + 4)

    for event_time, note, pan in events[start_index:end_index]:
        dt = time_seconds - event_time
        if dt < 0.0 or dt > 1.7:
            continue
        attack = smoothstep(dt / 0.08)
        decay = math.exp(-dt / 0.52)
        envelope = attack * decay
        freq = quantize_loop_frequency(NOTE_FREQS[note], duration)
        phase = note_phase(note) * 1.91
        tone = oscillator(freq, dt, phase)
        tone += 0.20 * oscillator(freq * 2.0, dt, phase * 0.7)
        value = tone * envelope * 0.030
        left += value * (1.0 - pan)
        right += value * (1.0 + pan)

    return left, right


def sample_at(
    time_seconds: float,
    duration: float,
    events: list[tuple[float, str, float]],
) -> tuple[float, float]:
    left = 0.0
    right = 0.0
    current_notes, current_weight, next_notes, next_weight = chord_blend(time_seconds)

    for note in current_notes:
        left, right = add_note(left, right, note, time_seconds, 0.012 * current_weight, duration)
    for note in next_notes:
        left, right = add_note(left, right, note, time_seconds, 0.012 * next_weight, duration)

    root_note = current_notes[0]
    bass_freq = quantize_loop_frequency(NOTE_FREQS[root_note] * 0.5, duration)
    bass = oscillator(bass_freq, time_seconds, note_phase(root_note)) * 0.018 * current_weight
    left += bass * 0.95
    right += bass * 1.05

    bell_left, bell_right = bell_sample(time_seconds, events, duration)
    left += bell_left
    right += bell_right

    edge_fade = min(
        smoothstep(time_seconds / 0.08),
        smoothstep((duration - time_seconds) / 0.08),
    )
    left *= MASTER_GAIN * edge_fade
    right *= MASTER_GAIN * edge_fade
    return left, right


def write_wav(path: Path, duration: float) -> None:
    events = bell_events(duration)
    total_samples = int(SAMPLE_RATE * duration)
    path.parent.mkdir(parents=True, exist_ok=True)
    with wave.open(str(path), "wb") as wav_file:
        wav_file.setnchannels(2)
        wav_file.setsampwidth(2)
        wav_file.setframerate(SAMPLE_RATE)
        frames = bytearray()
        for index in range(total_samples):
            left, right = sample_at(index / SAMPLE_RATE, duration, events)
            left_i = max(-32767, min(32767, int(left * 32767)))
            right_i = max(-32767, min(32767, int(right * 32767)))
            frames += struct.pack("<hh", left_i, right_i)
            if len(frames) >= SAMPLE_RATE * 4:
                wav_file.writeframes(frames)
                frames.clear()
        if frames:
            wav_file.writeframes(frames)


def encode_ogg(wav_path: Path, output_path: Path, ffmpeg: str) -> None:
    output_path.parent.mkdir(parents=True, exist_ok=True)
    subprocess.run(
        [
            ffmpeg,
            "-hide_banner",
            "-loglevel",
            "error",
            "-y",
            "-i",
            str(wav_path),
            "-c:a",
            "libvorbis",
            "-q:a",
            "4",
            str(output_path),
        ],
        check=True,
    )


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "output",
        nargs="?",
        default=str(project_root() / "assets/audio/bgm.ogg"),
        help="Output .ogg or .wav path.",
    )
    parser.add_argument("--duration", type=float, default=DEFAULT_DURATION)
    parser.add_argument("--ffmpeg", default=shutil.which("ffmpeg"))
    args = parser.parse_args()

    output_path = Path(args.output)
    if output_path.suffix.lower() == ".wav":
        write_wav(output_path, args.duration)
        return

    if not args.ffmpeg:
        raise SystemExit("ffmpeg is required for OGG output. Pass --ffmpeg or write a .wav file.")

    with tempfile.TemporaryDirectory() as tmp_dir:
        wav_path = Path(tmp_dir) / "bgm.wav"
        write_wav(wav_path, args.duration)
        encode_ogg(wav_path, output_path, args.ffmpeg)


if __name__ == "__main__":
    main()
