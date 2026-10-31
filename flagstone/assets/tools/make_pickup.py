"""Generate flagstone/assets/pickup.wav from arithmetic (standard library only).

Run: python3 -I flagstone/assets/tools/make_pickup.py
"""

import math
import struct
import wave
from pathlib import Path


def build_pickup(path):
    """A short pickup cue: a rising sine blip with a brief fade-in and fade-out."""
    rate = 22050
    length = 0.1
    fade = 0.01
    peak = 0.3
    count = int(rate * length)
    samples = []
    phase = 0.0
    for n in range(count):
        t = n / rate
        frequency = 660.0 + 880.0 * (t / length)
        phase += 2.0 * math.pi * frequency / rate
        envelope = min(1.0, t / fade, (length - t) / fade)
        value = max(-1.0, min(1.0, math.sin(phase) * max(0.0, envelope)))
        samples.append(int(value * 32767 * peak))
    with wave.open(str(path), "wb") as out:
        out.setnchannels(1)
        out.setsampwidth(2)
        out.setframerate(rate)
        out.writeframes(struct.pack(f"<{len(samples)}h", *samples))


def main():
    target = Path(__file__).resolve().parent.parent / "pickup.wav"
    build_pickup(target)


if __name__ == "__main__":
    main()
