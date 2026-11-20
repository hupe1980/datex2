"""Compares two JSON documents as DATEX II values: the second is the first as the datex2 writer
writes it back.

Usage: same_json.py <original.json> <written.json>

Allowed differences, and only these: `targetClass`, which the writer always puts on a reference;
empty arrays, which the writer leaves out; and the text of a number whose value is the same
(exactly as a decimal, or as the same single precision float, which is what DATEX II declares
for coordinates and measurements). Exit status 0 when the documents agree; otherwise the first
differences are printed.
"""

import json
import struct
import sys
from decimal import Decimal


def normal(value):
    if isinstance(value, dict):
        return {k: normal(v) for k, v in value.items() if k != "targetClass" and v != []}
    if isinstance(value, list):
        return [normal(v) for v in value]
    return value


def same_number(a, b):
    if Decimal(str(a)) == Decimal(str(b)):
        return True
    return struct.pack("<f", float(a)) == struct.pack("<f", float(b))


def differences(a, b, path, out):
    numbers = (int, float, Decimal)
    if isinstance(a, numbers) and isinstance(b, numbers) and not isinstance(a, bool) and not isinstance(b, bool):
        if not same_number(a, b):
            out.append(f"{path}: {a!r} != {b!r}")
    elif type(a) is not type(b):
        out.append(f"{path}: {a!r:.80} != {b!r:.80}")
    elif isinstance(a, dict):
        for key in sorted(set(a) | set(b)):
            if key not in a or key not in b:
                out.append(f"{path}/{key}: only in the {'original' if key in a else 'written'} document")
            else:
                differences(a[key], b[key], f"{path}/{key}", out)
    elif isinstance(a, list):
        if len(a) != len(b):
            out.append(f"{path}: {len(a)} entries != {len(b)}")
        for i, (x, y) in enumerate(zip(a, b)):
            differences(x, y, f"{path}/{i}", out)
    elif a != b:
        out.append(f"{path}: {a!r:.80} != {b!r:.80}")


def main() -> int:
    with open(sys.argv[1], encoding="utf-8") as f:
        original = normal(json.load(f, parse_float=Decimal))
    with open(sys.argv[2], encoding="utf-8") as f:
        written = normal(json.load(f, parse_float=Decimal))
    out = []
    differences(original, written, "", out)
    for line in out[:10]:
        print(line)
    return 1 if out else 0


if __name__ == "__main__":
    sys.exit(main())
