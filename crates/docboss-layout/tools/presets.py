"""Generate `crates/docboss-layout/src/presets.rs` from the DrawingML preset
shape definitions of ECMA-376 Part 1 (the presetShapeDefinitions.xml
electronic addendum of Annex D, inside OfficeOpenXML-DrawingMLGeometries.zip).

Usage, from the repository root, with the ECMA-376 Part 1 zip in ledger/specs
(see ledger/tools/outline.py for where to fetch it):

    python3 crates/docboss-layout/tools/presets.py \
        ledger/specs/ECMA-376-1_5th_edition_december_2016.zip \
        crates/docboss-layout/src/presets.rs

Every guide argument and path coordinate becomes a literal or a slot: the
built-in guides first, in the order of `BUILTINS` (mirrored by
`geometry::builtins`), then the shape's adjust values, then its guides. A
formula with more arguments than its operator takes (four `+-` guides of
the addendum) keeps the first three, and a shape defined twice the same
way (`upDownArrow`) is kept once.
"""

from __future__ import annotations

import io
import re
import sys
import xml.etree.ElementTree as ET
import zipfile

A = "{http://schemas.openxmlformats.org/drawingml/2006/main}"

BUILTINS = [
    "w", "h", "l", "t", "r", "b", "hc", "vc",
    "wd2", "wd3", "wd4", "wd5", "wd6", "wd8", "wd10", "wd12", "wd32",
    "hd2", "hd3", "hd4", "hd5", "hd6", "hd8", "hd10", "hd12", "hd32",
    "ss", "ls", "ssd2", "ssd4", "ssd6", "ssd8", "ssd16", "ssd32",
    "cd2", "cd4", "cd8", "3cd4", "3cd8", "5cd8", "7cd8",
]

OPS = {
    "*/": "MulDiv", "+-": "AddSub", "+/": "AddDiv", "?:": "IfElse",
    "abs": "Abs", "at2": "At2", "cat2": "Cat2", "cos": "Cos", "max": "Max",
    "min": "Min", "mod": "Mod", "pin": "Pin", "sat2": "Sat2", "sin": "Sin",
    "sqrt": "Sqrt", "tan": "Tan", "val": "Val",
}

FILLS = {
    "none": "None", "norm": "Normal", "lighten": "Lighten",
    "lightenLess": "LightenLess", "darken": "Darken", "darkenLess": "DarkenLess",
}


def definitions(zip_path: str) -> bytes:
    with zipfile.ZipFile(zip_path) as outer:
        inner = outer.read("OfficeOpenXML-DrawingMLGeometries.zip")
    with zipfile.ZipFile(io.BytesIO(inner)) as geometries:
        return geometries.read("presetShapeDefinitions.xml")


def number(text: str) -> str:
    value = float(text)
    return f"{value:.1f}" if value == int(value) else repr(value)


class Shape:
    def __init__(self, element: ET.Element):
        self.name = element.tag
        self.slots = {name: i for i, name in enumerate(BUILTINS)}
        self.adjust = []
        self.guides = []
        self.paths = []
        self.text = None
        for gd in element.findall(f"{A}avLst/{A}gd"):
            op, *args = gd.get("fmla").split()
            if op != "val" or len(args) != 1:
                raise ValueError(f"{self.name}: adjust {gd.get('name')} is not a literal")
            self.slots[gd.get("name")] = len(self.slots)
            self.adjust.append((gd.get("name"), number(args[0])))
        for gd in element.findall(f"{A}gdLst/{A}gd"):
            op, *args = gd.get("fmla").split()
            padded = [self.arg(a) for a in args[:3]] + ["L(0.0)"] * (3 - len(args[:3]))
            self.guides.append(f"G({OPS[op]}, [{', '.join(padded)}])")
            self.slots[gd.get("name")] = len(self.slots)
        rect = element.find(f"{A}rect")
        if rect is not None:
            self.text = [self.arg(rect.get(side)) for side in ("l", "t", "r", "b")]
        for path in element.findall(f"{A}pathLst/{A}path"):
            self.paths.append(self.path(path))

    def arg(self, token: str) -> str:
        if re.fullmatch(r"-?\d+(\.\d+)?", token):
            return f"L({number(token)})"
        if token not in self.slots:
            raise ValueError(f"{self.name}: unknown guide {token}")
        return f"S({self.slots[token]})"

    def point(self, element: ET.Element) -> list[str]:
        return [self.arg(element.get("x")), self.arg(element.get("y"))]

    def path(self, path: ET.Element) -> str:
        commands = []
        for command in path:
            tag = command.tag.removeprefix(A)
            points = [c for p in command.findall(f"{A}pt") for c in self.point(p)]
            if tag == "moveTo":
                commands.append(f"Move({', '.join(points)})")
            elif tag == "lnTo":
                commands.append(f"Line({', '.join(points)})")
            elif tag == "quadBezTo":
                commands.append(f"Quad({', '.join(points)})")
            elif tag == "cubicBezTo":
                commands.append(f"Cubic({', '.join(points)})")
            elif tag == "arcTo":
                args = [self.arg(command.get(k)) for k in ("wR", "hR", "stAng", "swAng")]
                commands.append(f"Arc({', '.join(args)})")
            elif tag == "close":
                commands.append("Close")
            else:
                raise ValueError(f"{self.name}: unknown path command {tag}")
        fill = FILLS[path.get("fill", "norm")]
        stroke = "true" if path.get("stroke", "true") in ("true", "1") else "false"
        width = number(path.get("w", "0"))
        height = number(path.get("h", "0"))
        return (
            f"P {{ width: {width}, height: {height}, fill: F::{fill}, stroke: {stroke}, "
            f"commands: &[{', '.join(commands)}] }}"
        )

    def rust(self) -> str:
        adjust = ", ".join(f'("{n}", {v})' for n, v in self.adjust)
        text = f"Some([{', '.join(self.text)}])" if self.text else "None"
        return (
            f'    Preset {{\n        name: "{self.name}",\n        adjust: &[{adjust}],\n'
            f"        guides: &[{', '.join(self.guides)}],\n"
            f"        paths: &[{', '.join(self.paths)}],\n        text: {text},\n    }},\n"
        )


def main() -> None:
    zip_path, out_path = sys.argv[1], sys.argv[2]
    root = ET.fromstring(definitions(zip_path))
    unique: dict[str, ET.Element] = {}
    for element in root:
        seen = unique.setdefault(element.tag, element)
        if ET.tostring(seen) != ET.tostring(element):
            raise ValueError(f"{element.tag} is defined twice, differently")
    shapes = sorted((Shape(e) for e in unique.values()), key=lambda s: s.name)
    with open(out_path, "w") as out:
        out.write(
            "//! The DrawingML preset shape geometries of ECMA-376 Part 1 §20.1.10.56,\n"
            "//! generated by tools/presets.py from presetShapeDefinitions.xml. Do not\n"
            "//! edit by hand.\n\n"
            "use crate::geometry::Arg::{Lit as L, Slot as S};\n"
            "use crate::geometry::Cmd::{Arc, Close, Cubic, Line, Move, Quad};\n"
            "use crate::geometry::Op::*;\n"
            "use crate::geometry::{Guide as G, Path as P, PathFill as F, Preset};\n\n"
            f"/// The {len(shapes)} presets, sorted by name.\n"
            "#[rustfmt::skip]\n"
            "pub(crate) static PRESETS: &[Preset] = &[\n"
        )
        for shape in shapes:
            out.write(shape.rust())
        out.write("];\n")


if __name__ == "__main__":
    main()
