"""Figures for the manuals: signal-flow charts (rects and arrows, drawn like the live flow on the About page) and the exploded-view
hero images (the screenshot with leader lines, module brackets and notes around it). Both return HTML (inline SVG for the lines,
HTML text on top) that WeasyPrint lays out on the black page. In a manual's markdown:

    <!--flow:satcomp-->        the signal-flow chart
    <!--hero:satcomp-->        the annotated screenshot

Coordinates of the hero are in "canvas pixels": the screenshot sits at (ix, iy) at its natural pixel size and the whole canvas is
scaled to the page width, so a point on the screenshot is the same number you read in an image editor plus (ix, iy).
"""
import pathlib
from PIL import Image

HERE = pathlib.Path(__file__).resolve().parent
SHOTS = HERE.parent / "screenshots"
ACC = "#b6ff3b"      # annotation ink (the plug-ins' own white and red stay the UI's)
BUILD = HERE / ".build"  # cropped screenshots (generated, git-ignored)
PAGE_W = 178.0       # mm, A4 less the margins

# ---------------------------------------------------------------------------------------------------- hero


def _svg(w, h, body):
    return f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {w} {h}" width="{w}" height="{h}" style="position:absolute;left:0;top:0;width:100%;height:100%">{body}</svg>'


def hero(spec):
    img = Image.open(SHOTS / spec["image"]).crop(spec["crop"])
    iw, ih = img.size
    BUILD.mkdir(exist_ok=True)
    cropped = BUILD / spec["image"]
    img.save(cropped)
    ix, iy = spec["at"]
    cw, ch = spec["canvas"]
    k = 100.0 / cw  # % per canvas px
    kh = 100.0 / ch
    svg, txt = [], []
    ink = f'stroke="{ACC}" stroke-width="2" fill="none" stroke-linecap="round" stroke-linejoin="round"'

    def label(x, y, w, text, align="l", size=1.0):
        a = {"l": "left", "c": "center", "r": "right"}[align]
        pad = 600  # labels never wrap: the box is widened on the side the text may grow into
        if align == "c":
            x, w = x - pad / 2, w + pad
        elif align == "r":
            x, w = x - pad, w + pad
        else:
            w = w + pad
        txt.append(
            f'<div style="position:absolute;left:{x * k:.3f}%;top:{y * kh:.3f}%;width:{w * k:.3f}%;text-align:{a};'
            f'white-space:nowrap;color:{ACC};font-size:{6.9 * size:.2f}pt;line-height:1.28">{text.replace(chr(10), "<br>")}</div>'
        )

    for a in spec.get("leaders", []):
        # (text, (lx, ly, lw, align), [points from the label to the target])
        text, (lx, ly, lw, al), pts = a
        label(lx, ly, lw, text, al)
        d = "M" + " L".join(f"{x} {y}" for x, y in pts)
        svg.append(f'<path d="{d}" {ink}/>')
        ex, ey = pts[-1]
        svg.append(f'<circle cx="{ex}" cy="{ey}" r="4.5" fill="{ACC}"/>')
    for a in spec.get("brackets", []):
        # (text, x0, x1, y, lw): a bracket under the screenshot opening upwards, a stem down to the label
        text, x0, x1, y = a[:4]
        lw = a[4] if len(a) > 4 else x1 - x0
        mid = (x0 + x1) / 2
        svg.append(f'<path d="M{x0} {y - 14} L{x0} {y} L{x1} {y} L{x1} {y - 14} M{mid} {y} L{mid} {y + 14}" {ink}/>')
        label(mid - lw / 2, y + 20, lw, text, "c", 1.15)
    for text, x, y, w in spec.get("notes", []):
        label(x, y, w, text, "l")
    return (
        f'<div class="hero" style="width:{PAGE_W}mm;height:{PAGE_W * ch / cw:.2f}mm">'
        f'<img src="{cropped.as_uri()}" style="position:absolute;left:{ix * k:.3f}%;top:{iy * kh:.3f}%;width:{iw * k:.3f}%;'
        f'height:{ih * kh:.3f}%;border:0;margin:0;max-height:none">'
        f'{_svg(cw, ch, "".join(svg))}{"".join(txt)}</div>'
    )


# ---------------------------------------------------------------------------------------------------- flow


def flow(spec):
    """spec: size (w, h), boxes {id: (x, y, w, h, label, style)}, arrows [(points, dashed, label)], notes [(x, y, w, text, align)].
    style: 0 outlined, 1 filled (a stage that colours the sound), 2 dim (optional / bypassable)."""
    W, H = spec["size"]
    out = []
    for pts, dashed, lab in spec["arrows"]:
        d = "M" + " L".join(f"{x} {y}" for x, y in pts)
        da = ' stroke-dasharray="5 4"' if dashed else ""
        out.append(f'<path d="{d}" stroke="#fff" stroke-width="1.5" fill="none"{da}/>')
        (x0, y0), (x1, y1) = pts[-2], pts[-1]
        if x1 != x0:
            s = 1 if x1 > x0 else -1
            out.append(f'<path d="M{x1} {y1} L{x1 - 8 * s} {y1 - 4.5} L{x1 - 8 * s} {y1 + 4.5} Z" fill="#fff"/>')
        else:
            s = 1 if y1 > y0 else -1
            out.append(f'<path d="M{x1} {y1} L{x1 - 4.5} {y1 - 8 * s} L{x1 + 4.5} {y1 - 8 * s} Z" fill="#fff"/>')
    txt = []
    for bid, (x, y, w, h, lab, st) in spec["boxes"].items():
        fill = "#fff" if st == 1 else "#000"
        col = "#000" if st == 1 else "#fff"
        dash = ' stroke-dasharray="4 3"' if st == 2 else ""
        out.append(f'<rect x="{x}" y="{y}" width="{w}" height="{h}" fill="{fill}" stroke="#fff" stroke-width="1.5"{dash}/>')
        lines = lab.split("\n")
        for i, ln in enumerate(lines):
            ty = y + h / 2 + (i - (len(lines) - 1) / 2) * 12 + 4
            out.append(f'<text x="{x + w / 2}" y="{ty}" fill="{col}" text-anchor="middle" font-family="Fira Code" font-size="10.5" font-weight="700">{ln}</text>')
    for x, y, w, text, al in spec.get("notes", []):
        lines = text.split("\n")
        for i, ln in enumerate(lines):
            tx = x + (w / 2 if al == "c" else (w if al == "r" else 0))
            anchor = {"l": "start", "c": "middle", "r": "end"}[al]
            out.append(f'<text x="{tx}" y="{y + i * 11.5}" fill="#bbb" text-anchor="{anchor}" font-family="Fira Code" font-size="8.6">{ln}</text>')
    return f'<div class="flow" style="width:{PAGE_W}mm;height:{PAGE_W * H / W:.2f}mm"><svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {W} {H}" style="width:100%;height:100%">{"".join(out)}</svg></div>'


def row(names, x0, y, widths, gap, h=30, styles=None):
    """Helper: a left-to-right row of boxes with an arrow between each. Returns (boxes, arrows, xs)."""
    boxes, arrows, x = {}, [], x0
    xs = []
    for i, (nm, w) in enumerate(zip(names, widths)):
        st = (styles or {}).get(nm, 0)
        boxes[f"{nm}{i}"] = (x, y, w, h, nm, st)
        xs.append((x, w))
        if i:
            arrows.append(([(x - gap, y + h / 2), (x, y + h / 2)], False, ""))
        x += w + gap
    return boxes, arrows, xs


from figure_specs import FLOWS, HEROES  # noqa: E402


def expand(html):
    import re

    def sub(m):
        kind, name = m.group(1), m.group(2)
        return (flow(FLOWS[name]) if kind == "flow" else hero(HEROES[name]))

    return re.sub(r"<!--(flow|hero):(\w+)-->", sub, html)
