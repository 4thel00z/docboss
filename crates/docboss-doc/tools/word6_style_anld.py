"""Makes word6-style-anld.doc from Apache POI's Bug60942.doc: two body
paragraphs (fc 1076 and 1227) switched to istd 17, the paragraph style
"carre+" whose ANLD bullets them. Needs olefile.

    python3 word6_style_anld.py Bug60942.doc word6-style-anld.doc
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
count = w[base + 511]
at = 428
w[base + at:base + at + 3] = bytes([1, 17, 0])
for i in range(count):
    start = struct.unpack_from("<I", w, base + i * 4)[0]
    if start in (1076, 1227):
        w[base + (count + 1) * 4 + i * 7] = at // 2
ole.write_stream("WordDocument", bytes(w))
ole.close()
print("paragraphs at fc 1076 and 1227 now use istd 17 (carre+)")
