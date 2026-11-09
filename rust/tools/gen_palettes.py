#!/usr/bin/env python3
"""Rewrites the PALETTES table in satcomp-plugin/src/gui/canvas.rs from docs/colorscheme.md. Run from rust/."""
import re
t = open("../docs/colorscheme.md").read()
th = []
for m in re.finditer(r"^(\S[^\n]*)\nDark\s+BG: (#\w{6})\s+UI: (#\w{6})\s+ACCENT: (#\w{6})\nLight\s+BG: (#\w{6})\s+UI: (#\w{6})\s+ACCENT: (#\w{6})", t, re.M):
    g = m.groups()
    th.append((g[0].strip(), g[1:4], g[4:7]))
def fl(h):
    out = []
    for c in h:
        out += [round(int(c[i:i + 2], 16) / 255, 4) for i in (1, 3, 5)]
    return "[" + ", ".join("%.1f" % v if v in (0.0, 1.0) else repr(v) for v in out) + "]"
rs = f"pub const PALETTES: [(&str, [f32; 9], [f32; 9]); {len(th)}] = [\n" + "".join(f'    ("{n}", {fl(d)}, {fl(l)}),\n' for n, d, l in th) + "];\n"
f = "satcomp-plugin/src/gui/canvas.rs"
c = open(f).read()
a = c.index("pub const PALETTES"); b = c.index("];\n", a) + 3
open(f, "w").write(c[:a] + rs + c[b:])
print(len(th), "schemes")
