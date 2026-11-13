"""Generate flagstone/assets/dig.wav from arithmetic (standard library only).

Run: python3 -I flagstone/assets/tools/make_dig.py
"""

import math
import struct
import wave
from pathlib import Path


def build_dig(path):
    """A short dig cue: a falling thud with a burst of noise from a fixed generator."""
    rate = 22050
    length = 0.15
    peak = 0.4
    count = int(rate * length)
    samples = []
    phase = 0.0
    state = 12345
    for n in range(count):
        t = n / rate
        frequency = 180.0 - 100.0 * (t / length)
        phase += 2.0 * math.pi * frequency / rate
        state = (state * 1103515245 + 12345) % 2**31
        noise = state / 2**30 - 1.0
        envelope = min(1.0, t / 0.003) * math.exp(-t * 28.0)
        value = (0.8 * math.sin(phase) + 0.2 * noise) * envelope
        samples.append(int(max(-1.0, min(1.0, value)) * 32767 * peak))
    with wave.open(str(path), "wb") as out:
        out.setnchannels(1)
        out.setsampwidth(2)
        out.setframerate(rate)
        out.writeframes(struct.pack(f"<{len(samples)}h", *samples))


def main():
    target = Path(__file__).resolve().parent.parent / "dig.wav"
    build_dig(target)


if __name__ == "__main__":
    main()
