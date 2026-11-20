"""Generate flagstone/assets/dig.wav: a short decaying low thud.

Standard library only; the output is deterministic arithmetic, no third-party
sample is used or derived from. Run from the repository root:

    python3 -I flagstone/assets/tools/make_dig.py
"""

import math
import struct
import wave
from pathlib import Path

RATE = 22_050
SECONDS = 0.12
START_HZ = 140.0
END_HZ = 70.0
DECAY = 28.0

OUT = Path(__file__).resolve().parent.parent / "dig.wav"


def main() -> None:
    count = int(RATE * SECONDS)
    frames = bytearray()
    phase = 0.0
    for n in range(count):
        t = n / RATE
        hz = START_HZ + (END_HZ - START_HZ) * (n / count)
        phase += 2.0 * math.pi * hz / RATE
        level = math.sin(phase) * math.exp(-DECAY * t)
        frames += struct.pack("<h", int(level * 0.8 * 32767))
    with wave.open(str(OUT), "wb") as wav:
        wav.setnchannels(1)
        wav.setsampwidth(2)
        wav.setframerate(RATE)
        wav.writeframes(bytes(frames))


if __name__ == "__main__":
    main()
