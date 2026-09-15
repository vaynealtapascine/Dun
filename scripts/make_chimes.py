"""Synthesizes Dun's bundled chimes as 16-bit mono WAV files.

Generated rather than downloaded so every sound is original and license-free.
Writes the same files to the desktop resources and the Android plugin's raw
resources (Android resource names must be lowercase with underscores).

    python scripts/make_chimes.py
"""

import math
import struct
import wave
from pathlib import Path

RATE = 44_100
ROOT = Path(__file__).resolve().parent.parent
OUT_DIRS = [
    ROOT / "src-tauri" / "resources" / "chimes",
    ROOT / "plugins" / "tauri-plugin-dun-android" / "android" / "src" / "main" / "res" / "raw",
]


def note(freq, start, length, gain=1.0, decay=4.0, partials=((1, 1.0),), attack=0.004):
    """A struck tone: sine partials with an exponential decay."""
    return (freq, start, length, gain, decay, partials, attack)


def render(duration, notes):
    samples = [0.0] * int(duration * RATE)
    for freq, start, length, gain, decay, partials, attack in notes:
        first = int(start * RATE)
        count = min(int(length * RATE), len(samples) - first)
        for i in range(count):
            t = i / RATE
            env = math.exp(-decay * t) * min(1.0, t / attack)
            value = sum(amp * math.sin(2 * math.pi * freq * mult * t) for mult, amp in partials)
            samples[first + i] += gain * env * value
    peak = max(abs(s) for s in samples) or 1.0
    scale = 0.85 / peak
    # Short fade-out so the file never ends on a click.
    fade = int(0.02 * RATE)
    for i in range(fade):
        samples[-1 - i] *= i / fade
    return [int(max(-1.0, min(1.0, s * scale)) * 32767) for s in samples]


BELL = ((1, 1.0), (2.76, 0.45), (5.4, 0.2), (8.93, 0.08))
MARIMBA = ((1, 1.0), (4.0, 0.25), (10.0, 0.05))
GLASS = ((1, 1.0), (2.0, 0.3), (3.0, 0.12))

CHIMES = {
    # A clear two-strike bell.
    "bell": render(1.6, [note(880, 0.0, 1.6, decay=3.2, partials=BELL), note(1318.5, 0.18, 1.4, 0.8, 3.2, BELL)]),
    # Three rising notes.
    "rise": render(1.4, [
        note(659.3, 0.00, 0.9, decay=5, partials=GLASS),
        note(830.6, 0.16, 0.9, decay=5, partials=GLASS),
        note(987.8, 0.32, 1.08, decay=4, partials=GLASS),
    ]),
    # Two short, insistent pulses: hard to ignore.
    "pulse": render(0.9, [
        note(1046.5, 0.00, 0.22, decay=14, partials=((1, 1.0), (3, 0.3))),
        note(1046.5, 0.28, 0.22, decay=14, partials=((1, 1.0), (3, 0.3))),
        note(1318.5, 0.56, 0.32, decay=10, partials=((1, 1.0), (3, 0.3))),
    ]),
    # Soft wooden tones for quieter rooms.
    "soft": render(1.2, [note(523.3, 0.0, 1.2, 0.9, 6, MARIMBA), note(783.99, 0.14, 1.0, 0.7, 6, MARIMBA)]),
}


def write(path, samples):
    with wave.open(str(path), "wb") as w:
        w.setnchannels(1)
        w.setsampwidth(2)
        w.setframerate(RATE)
        w.writeframes(struct.pack(f"<{len(samples)}h", *samples))


def main():
    for out in OUT_DIRS:
        out.mkdir(parents=True, exist_ok=True)
        for name, samples in CHIMES.items():
            target = out / (f"chime_{name}.wav" if "res" in out.parts else f"{name}.wav")
            write(target, samples)
            print(f"{target.relative_to(ROOT)}  {target.stat().st_size // 1024} KB")


if __name__ == "__main__":
    main()
