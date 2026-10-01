"""Makes word6-anld.doc from Apache POI's Bug60942.doc: three body
paragraphs, split by empty ones, numbered by a decimal ANLD with "." after
(sprmPNLvlAnm 10), and one bulleted by a Word 6.0 ANLD (nfc 255, Wingdings,
sprmPNLvlAnm 11), in the PAPX FKP at sector 6. Needs olefile.

    python3 word6_anld.py Bug60942.doc word6-anld.doc
"""

import shutil
import struct
import sys

import olefile

src, dst = sys.argv[1], sys.argv[2]
shutil.copy(src, dst)
ole = olefile.OleFileIO(dst, write_mode=True)
w = bytearray(ole.openstream("WordDocument").read())
base = 6 * 512
page = bytes(w[base:base + 512])
count = page[511]
fcs = [struct.unpack_from("<I", page, i * 4)[0] for i in range(count + 1)]
bxs = [bytearray(page[(count + 1) * 4 + i * 7:(count + 1) * 4 + i * 7 + 7]) for i in range(count)]
runs = [[fcs[i], fcs[i + 1], bxs[i]] for i in range(count)]


def merge(runs: list, a: int, b: int) -> list:
    return runs[:a] + [[runs[a][0], runs[b][1], runs[a][2]]] + runs[b + 1:]


runs = merge(runs, 20, 26)
runs = merge(runs, 7, 10)
style_anld = bytes.fromhex("ff01000800000300000001001b01000000000000" + "6e" + "00" * 31)
numbered = bytearray(52)
numbered[2] = 1
numbered[3] = 0x08
numbered[10] = 1
numbered[12:14] = b"\x1b\x01"
numbered[20] = ord(".")


def papx(level: int, anld: bytes) -> bytes:
    grpprl = bytes([13, level, 12, 52]) + anld
    body = b"\x00\x00" + grpprl
    return bytes([len(body) // 2]) + body


n_at, b_at = 372, 312
n_papx, b_papx = papx(10, bytes(numbered)), papx(11, style_anld)
new_count = len(runs)
arrays_end = (new_count + 1) * 4 + new_count * 7
if arrays_end > b_at or n_at + len(n_papx) > 432 or b_at + len(b_papx) > n_at:
    raise RuntimeError(f"no room: arrays end {arrays_end}")
targets = {1064: n_at, 1076: n_at, 1227: n_at, 1318: b_at}
out = bytearray(512)
out[432:511] = page[432:511]
out[n_at:n_at + len(n_papx)] = n_papx
out[b_at:b_at + len(b_papx)] = b_papx
for i, (start, end, bx) in enumerate(runs):
    struct.pack_into("<I", out, i * 4, start)
    if start in targets:
        bx[0] = targets[start] // 2
    out[(new_count + 1) * 4 + i * 7:(new_count + 1) * 4 + i * 7 + 7] = bx
struct.pack_into("<I", out, new_count * 4, runs[-1][1])
out[511] = new_count
w[base:base + 512] = out
ole.write_stream("WordDocument", bytes(w))
ole.close()
print("runs", count, "->", new_count, "arrays end", arrays_end, "patched", sorted(targets))
