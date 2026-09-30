"""Write an XOR-obfuscated copy of a Word 97-2003 document.

Usage, from the repository root:

    python3 crates/docboss-doc/tools/xor_obfuscate.py \
        crates/docboss-doc/tests/fixtures/text.doc docboss \
        crates/docboss-doc/tests/fixtures/encrypted-xor.doc

Follows [MS-DOC] §2.2.6.1: FibBase.fEncrypted and fObfuscated are set,
FibBase.lKey takes the method 2 password verifier, and the WordDocument
stream past its first 68 bytes, the table stream and the Data stream are
transformed with XOR data transformation method 2 of [MS-OFFCRYPTO]
(sections 2.3.7.1, 2.3.7.2, 2.3.7.4, 2.3.7.5 and 2.3.7.6). The compound
file is patched in place, so every stream keeps its sectors and size.
"""

from __future__ import annotations

import struct
import sys

PAD = [0xBB, 0xFF, 0xFF, 0xBA, 0xFF, 0xFF, 0xB9, 0x80, 0x00, 0xBE, 0x0F, 0x00, 0xBF, 0x0F, 0x00]
INITIAL_CODE = [
    0xE1F0, 0x1D0F, 0xCC9C, 0x84C0, 0x110C, 0x0E10, 0xF1CE, 0x313E,
    0x1872, 0xE139, 0xD40F, 0x84F9, 0x280C, 0xA96A, 0x4EC3,
]
XOR_MATRIX = [
    0xAEFC, 0x4DD9, 0x9BB2, 0x2745, 0x4E8A, 0x9D14, 0x2A09,
    0x7B61, 0xF6C2, 0xFDA5, 0xEB6B, 0xC6F7, 0x9DCF, 0x2BBF,
    0x4563, 0x8AC6, 0x05AD, 0x0B5A, 0x16B4, 0x2D68, 0x5AD0,
    0x0375, 0x06EA, 0x0DD4, 0x1BA8, 0x3750, 0x6EA0, 0xDD40,
    0xD849, 0xA0B3, 0x5147, 0xA28E, 0x553D, 0xAA7A, 0x44D5,
    0x6F45, 0xDE8A, 0xAD35, 0x4A4B, 0x9496, 0x390D, 0x721A,
    0xEB23, 0xC667, 0x9CEF, 0x29FF, 0x53FE, 0xA7FC, 0x5FD9,
    0x47D3, 0x8FA6, 0x0F6D, 0x1EDA, 0x3DB4, 0x7B68, 0xF6D0,
    0xB861, 0x60E3, 0xC1C6, 0x93AD, 0x377B, 0x6EF6, 0xDDEC,
    0x45A0, 0x8B40, 0x06A1, 0x0D42, 0x1A84, 0x3508, 0x6A10,
    0xAA51, 0x4483, 0x8906, 0x022D, 0x045A, 0x08B4, 0x1168,
    0x76B4, 0xED68, 0xCAF1, 0x85C3, 0x1BA7, 0x374E, 0x6E9C,
    0x3730, 0x6E60, 0xDCC0, 0xA9A1, 0x4363, 0x86C6, 0x1DAD,
    0x3331, 0x6662, 0xCCC4, 0x89A9, 0x0373, 0x06E6, 0x0DCC,
    0x1021, 0x2042, 0x4084, 0x8108, 0x1231, 0x2462, 0x48C4,
]


def verifier1(password: bytes) -> int:
    verifier = 0
    for byte in reversed(bytes([len(password)]) + password):
        intermediate1 = 1 if verifier & 0x4000 else 0
        intermediate2 = (verifier * 2) & 0x7FFF
        verifier = (intermediate1 | intermediate2) ^ byte
    return verifier ^ 0xCE4B


def xor_key(password: bytes) -> int:
    key = INITIAL_CODE[len(password) - 1]
    element = 0x68
    for char in reversed(password):
        for _ in range(7):
            if char & 0x40:
                key ^= XOR_MATRIX[element]
            char = (char * 2) & 0xFF
            element -= 1
    return key


def ror(byte: int) -> int:
    return ((byte >> 1) | (byte << 7)) & 0xFF


def xor_array(password: bytes) -> list[int]:
    key = xor_key(password)
    high, low = key >> 8, key & 0xFF
    array = list(password) + PAD[: 16 - len(password)]
    return [ror(b ^ (low if i % 2 == 0 else high)) for i, b in enumerate(array)]


class Cfb:
    def __init__(self, data: bytearray):
        self.data = data
        self.sector = 1 << struct.unpack_from("<H", data, 0x1E)[0]
        self.mini = 1 << struct.unpack_from("<H", data, 0x20)[0]
        self.cutoff = struct.unpack_from("<I", data, 0x38)[0]
        difat = list(struct.unpack_from("<109I", data, 0x4C))
        next_difat = struct.unpack_from("<I", data, 0x44)[0]
        while next_difat < 0xFFFFFFFA:
            words = struct.unpack_from(f"<{self.sector // 4}I", data, self.offset(next_difat))
            difat += words[:-1]
            next_difat = words[-1]
        self.fat: list[int] = []
        for sector in difat:
            if sector >= 0xFFFFFFFA:
                continue
            self.fat += struct.unpack_from(f"<{self.sector // 4}I", data, self.offset(sector))
        mini_fat_start = struct.unpack_from("<I", data, 0x3C)[0]
        self.mini_fat: list[int] = []
        for sector in self.chain(mini_fat_start):
            self.mini_fat += struct.unpack_from(f"<{self.sector // 4}I", data, self.offset(sector))
        self.entries = {}
        for sector in self.chain(struct.unpack_from("<I", data, 0x30)[0]):
            base = self.offset(sector)
            for at in range(base, base + self.sector, 128):
                length = struct.unpack_from("<H", data, at + 0x40)[0]
                name = bytes(data[at : at + max(length - 2, 0)]).decode("utf-16-le")
                start, size = struct.unpack_from("<IQ", data, at + 0x74)
                self.entries[name] = (start, size)
        self.root_sectors = self.chain(self.entries["Root Entry"][0])

    def offset(self, sector: int) -> int:
        return (sector + 1) * self.sector

    def chain(self, sector: int) -> list[int]:
        out = []
        while sector < 0xFFFFFFFA and len(out) <= len(self.fat):
            out.append(sector)
            sector = self.fat[sector]
        return out

    def byte_offsets(self, name: str) -> list[int]:
        start, size = self.entries[name]
        out = []
        if size >= self.cutoff:
            for sector in self.chain(start):
                out += range(self.offset(sector), self.offset(sector) + self.sector)
            return out[:size]
        sector = start
        while sector < 0xFFFFFFFA and len(out) < size:
            position = sector * self.mini
            container = self.root_sectors[position // self.sector]
            base = self.offset(container) + position % self.sector
            out += range(base, base + self.mini)
            sector = self.mini_fat[sector]
        return out[:size]


def main() -> None:
    source, password, target = sys.argv[1], sys.argv[2].encode("latin-1")[:15], sys.argv[3]
    data = bytearray(open(source, "rb").read())
    cfb = Cfb(data)
    word = cfb.byte_offsets("WordDocument")
    flags = data[word[10]] | data[word[11]] << 8
    table = "1Table" if flags & 0x0200 else "0Table"
    array = xor_array(password)
    streams = [("WordDocument", 68), (table, 0)]
    if "Data" in cfb.entries:
        streams.append(("Data", 0))
    for name, clear in streams:
        for i, at in enumerate(cfb.byte_offsets(name)):
            key = array[i % 16]
            if i >= clear and data[at] != 0 and data[at] != key:
                data[at] ^= key
    flags |= 0x0100 | 0x8000
    data[word[10]], data[word[11]] = flags & 0xFF, flags >> 8
    verifier = xor_key(password) << 16 | verifier1(password)
    for i, byte in enumerate(struct.pack("<I", verifier)):
        data[word[14 + i]] = byte
    open(target, "wb").write(data)


if __name__ == "__main__":
    main()
