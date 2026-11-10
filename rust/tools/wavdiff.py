#!/usr/bin/env python3
"""Null-test two WAV renders (e.g. the same loop rendered through two plug-in versions).

    python3 tools/wavdiff.py a.wav b.wav

Reads PCM 16/24/32-bit and 32/64-bit float, any channel count. Reports the length, the peak and RMS of the
difference (in dBFS) and the sample position of the largest difference. A bit-identical render prints "IDENTICAL".
Renders must start at the same project position and use the same sample rate / bit depth; shorter file is compared
over its length. No dependencies beyond the standard library.
"""
import math
import struct
import sys


def read_wav(path):
    data = open(path, "rb").read()
    if data[:4] != b"RIFF" or data[8:12] != b"WAVE":
        sys.exit(f"{path}: not a RIFF/WAVE file")
    pos, fmt, pcm = 12, None, None
    while pos + 8 <= len(data):
        cid, size = data[pos : pos + 4], struct.unpack("<I", data[pos + 4 : pos + 8])[0]
        body = data[pos + 8 : pos + 8 + size]
        if cid == b"fmt ":
            tag, ch, sr, _, _, bits = struct.unpack("<HHIIHH", body[:16])
            if tag == 0xFFFE and len(body) >= 26:  # WAVE_FORMAT_EXTENSIBLE: real tag is the first GUID word
                tag = struct.unpack("<H", body[24:26])[0]
            fmt = (tag, ch, sr, bits)
        elif cid == b"data":
            pcm = body
            break
        pos += 8 + size + (size & 1)
    if fmt is None or pcm is None:
        sys.exit(f"{path}: missing fmt or data chunk")
    tag, ch, sr, bits = fmt
    n = len(pcm) // (bits // 8)
    if tag == 1 and bits == 16:
        v = [x / 32768.0 for x in struct.unpack(f"<{n}h", pcm[: n * 2])]
    elif tag == 1 and bits == 24:
        v = []
        for i in range(n):
            b = pcm[i * 3 : i * 3 + 3]
            v.append(int.from_bytes(b, "little", signed=True) / 8388608.0)
    elif tag == 1 and bits == 32:
        v = [x / 2147483648.0 for x in struct.unpack(f"<{n}i", pcm[: n * 4])]
    elif tag == 3 and bits == 32:
        v = list(struct.unpack(f"<{n}f", pcm[: n * 4]))
    elif tag == 3 and bits == 64:
        v = list(struct.unpack(f"<{n}d", pcm[: n * 8]))
    else:
        sys.exit(f"{path}: unsupported format tag {tag}, {bits} bit")
    return v, ch, sr, bits


def db(x):
    return -999.0 if x <= 0 else 20 * math.log10(x)


def main():
    if len(sys.argv) != 3:
        sys.exit(__doc__)
    a, cha, sra, ba = read_wav(sys.argv[1])
    b, chb, srb, bb = read_wav(sys.argv[2])
    if (cha, sra) != (chb, srb):
        sys.exit(f"format mismatch: {cha}ch {sra} Hz vs {chb}ch {srb} Hz")
    n = min(len(a), len(b))
    if len(a) != len(b):
        print(f"note: lengths differ ({len(a) // cha} vs {len(b) // chb} frames), comparing the first {n // cha}")
    peak, sq, at = 0.0, 0.0, 0
    for i in range(n):
        d = abs(a[i] - b[i])
        sq += d * d
        if d > peak:
            peak, at = d, i
    ref = max((abs(x) for x in a[:n]), default=0.0)
    rms = math.sqrt(sq / max(n, 1))
    print(f"{n // cha} frames, {cha} ch, {sra} Hz ({ba}-bit vs {bb}-bit), signal peak {db(ref):.1f} dBFS")
    if peak == 0.0:
        print("IDENTICAL")
    else:
        print(f"difference: peak {db(peak):.1f} dBFS, RMS {db(rms):.1f} dBFS, largest at frame {at // cha} (ch {at % cha + 1}), {peak:.3e}")
        print("(below about -140 dBFS is f32 rounding noise; 24-bit renders quantise at about -138 dBFS)")


main()
