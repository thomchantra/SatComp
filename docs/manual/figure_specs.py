"""The data behind the manual figures (see figures.py): signal-flow charts and exploded-view hero annotations."""

# =================================================================================================== signal flows
# boxes: id -> (x, y, w, h, label, style)   style 0 outlined, 1 filled (shapes the sound), 2 dashed (optional stage)
# arrows: ([points], dashed, label)         the arrowhead is drawn at the last point


def _row(spec, names, x0, y, widths, gap, h=30, filled=(), dashed=()):
    x = x0
    prev = None
    for i, (nm, w) in enumerate(zip(names, widths)):
        st = 1 if nm in filled else (2 if nm in dashed else 0)
        spec["boxes"][f"b{len(spec['boxes'])}"] = (x, y, w, h, nm, st)
        if prev is not None:
            spec["arrows"].append(([(prev, y + h / 2), (x, y + h / 2)], False, ""))
        prev = x + w
        x += w + gap
    return x - gap


LEGEND = "filled = shapes the sound  |  dashed = optional, can be switched off"

# ---- SatComp
sc = {"size": (760, 215), "boxes": {}, "arrows": [], "notes": []}
names = ["IN", "SAT", "PRE EQ", "COMP", "MIX", "POST EQ", "OUT GAIN", "LIM", "OUT"]
widths = [40, 46, 64, 56, 46, 72, 82, 46, 46]
gap = 28
x0 = 14
y = 76
xs, x = [], x0
for w in widths:
    xs.append((x, w))
    x += w + gap
_row(sc, names, x0, y, widths, gap, filled=("SAT", "COMP", "LIM"), dashed=("PRE EQ", "POST EQ"))
# dry path around COMP: leaves after PRE EQ, runs under COMP and rejoins before MIX
xa = xs[2][0] + xs[2][1] + gap / 2
xb = xs[3][0] + xs[3][1] + gap / 2
sc["arrows"].append(([(xa, y + 15), (xa, y + 62), (xb, y + 62), (xb, y + 15)], True, "DRY"))
sc["notes"].append((xa, y + 78, xb - xa, "DRY (MIX knob)", "c"))
# sidechain key
cx = xs[3][0] + xs[3][1] / 2
sc["boxes"]["sck"] = (cx - 70, 8, 140, 28, "SIDECHAIN IN\nGAIN > HP > LP", 2)
sc["arrows"].append(([(cx, 36), (cx, y)], True, ""))
sc["notes"].append((cx + 80, 26, 170, "key for the detector", "l"))
sc["notes"].append((14, 200, 700, LEGEND, "l"))

# ---- SatXComp
H = 262
xc = {"size": (770, H), "boxes": {}, "arrows": [], "notes": []}
cy = 134
xc["boxes"]["in"] = (10, cy - 15, 38, 30, "IN", 0)
xc["boxes"]["xo"] = (66, cy - 15, 72, 30, "X-OVER", 1)
xc["arrows"].append(([(48, cy), (66, cy)], False, ""))
band_x, band_w = 190, 150
ys = [cy - 78 + i * 52 for i in range(4)]
bus1, bus2 = 164, 360
for i, (nm, yy) in enumerate(zip(["LO", "ML", "MH", "HI"], ys)):
    xc["boxes"][f"b{i}"] = (band_x, yy, band_w, 28, f"{nm}  SAT > COMP", 1)
    xc["arrows"].append(([(138, cy), (bus1, cy), (bus1, yy + 14), (band_x, yy + 14)], False, ""))
    xc["arrows"].append(([(band_x + band_w, yy + 14), (bus2, yy + 14), (bus2, cy), (382, cy)], False, ""))
xc["boxes"]["sum"] = (382, cy - 15, 44, 30, "SUM", 0)
x, prev = 444, 426
for nm, w in [("EQ", 34), ("MIX", 40), ("OUT GAIN", 76), ("LIM", 40), ("OUT", 40)]:
    xc["arrows"].append(([(prev, cy), (x, cy)], False, ""))
    xc["boxes"][f"r{nm}"] = (x, cy - 15, w, 30, nm, 1 if nm == "LIM" else (2 if nm == "EQ" else 0))
    prev = x + w
    x += w + 18
kx = band_x + band_w + 40
xc["boxes"]["sck"] = (kx, 6, 120, 24, "SIDECHAIN IN", 2)
xc["arrows"].append(([(kx + 60, 30), (kx + 60, ys[0] + 6), (band_x + band_w, ys[0] + 6)], True, ""))
xc["notes"].append((kx + 130, 22, 200, "key for the bands' detectors", "l"))
xc["notes"].append((10, H - 8, 740, LEGEND + "  |  each band: SAT, then COMP (down, up or both)", "l"))

# ---- SatLim
sl = {"size": (760, 150), "boxes": {}, "arrows": [], "notes": []}
names = ["IN", "SAT", "LIFT", "CLIP", "LIMITER", "OUT GAIN", "OUT"]
widths = [40, 62, 56, 56, 84, 82, 46]
xe = _row(sl, names, 14, 40, widths, 38, filled=("SAT", "CLIP", "LIMITER"))
sl["notes"].append((100, 100, 300, "x1 / x2 / x4 / x8 oversampled", "l"))
sl["notes"].append((14, 136, 720, "LIFT = CEILING minus THRESHOLD, in dB  |  filled = shapes the sound", "l"))

# ---- SatDiff
H = 205
sd = {"size": (800, H), "boxes": {}, "arrows": [], "notes": []}
cy = 84
x = 12
seq = [("IN GAIN", 70, 0), ("PHASE", 52, 0), ("COLOR FX", 74, 2)]
last = None
for nm, w, st in seq:
    sd["boxes"][nm + "1"] = (x, cy - 15, w, 30, nm, st)
    if last is not None:
        sd["arrows"].append(([(last, cy), (x, cy)], False, ""))
    last = x + w
    x += w + 22
# scene A / B stacked, then MORPH
ax = x
sd["boxes"]["A"] = (ax, cy - 38, 86, 28, "SCENE A", 1)
sd["boxes"]["B"] = (ax, cy + 10, 86, 28, "SCENE B", 1)
sd["arrows"].append(([(last, cy), (ax - 11, cy), (ax - 11, cy - 24), (ax, cy - 24)], False, ""))
sd["arrows"].append(([(ax - 11, cy), (ax - 11, cy + 24), (ax, cy + 24)], False, ""))
mx = ax + 86 + 22
sd["boxes"]["M"] = (mx, cy - 15, 62, 30, "MORPH", 0)
sd["arrows"].append(([(ax + 86, cy - 24), (mx - 11, cy - 24), (mx - 11, cy), (mx, cy)], False, ""))
sd["arrows"].append(([(ax + 86, cy + 24), (mx - 11, cy + 24), (mx - 11, cy)], False, ""))
x = mx + 62 + 22
last = mx + 62
for nm, w, st in [("COLOR FX", 74, 2), ("OUT HP", 62, 2), ("OUT GAIN", 78, 0), ("LIMIT", 52, 2), ("OUT", 40, 0)]:
    sd["boxes"][nm + "2"] = (x, cy - 15, w, 30, nm, st)
    sd["arrows"].append(([(last, cy), (x, cy)], False, ""))
    last = x + w
    x += w + 22
sd["notes"].append((ax - 6, cy + 54, 104, "A | B tab edits one;", "l"))
sd["notes"].append((ax - 6, cy + 66, 140, "MORPH decides what you hear", "l"))
sd["notes"].append((12, cy + 54, 190, "COLOR FX: before or after the", "l"))
sd["notes"].append((12, cy + 66, 190, "saturators (PRE-SAT / POST-SAT)", "l"))
sd["notes"].append((12, H - 12, 760, "filled = shapes the sound  |  dashed = optional, can be switched off  |  SAT MIX blends clean and saturated", "l"))

FLOWS = {"satcomp": sc, "satxcomp": xc, "satlim": sl, "satdiff": sd}

# =================================================================================================== exploded heroes
# Targets are SCREENSHOT pixels (what an image editor shows on docs/screenshots/*.png). The drop shadow around the window is
# cropped away. Labels: top(...) sits above the window (its line drops down), left(...) / right(...) sit in the side margins.
import pathlib
from PIL import Image

_SHOTS = pathlib.Path(__file__).resolve().parent.parent / "screenshots"
FS = 6.9  # label font, pt (figures.py)


class _H:
    def __init__(self, image, ml, mr, mt, mb):
        im = Image.open(_SHOTS / image)
        bb = im.split()[-1].point(lambda v: 255 if v > 100 else 0).getbbox()
        self.image, self.ox, self.oy = image, bb[0], bb[1]
        self.iw, self.ih = bb[2] - bb[0], bb[3] - bb[1]
        self.ml, self.mr, self.mt, self.mb = ml, mr, mt, mb
        self.cw, self.ch = self.iw + ml + mr, self.ih + mt + mb
        self.lh = self.cw * 0.01367 * 1.28 * FS / 6.9
        self.gapr = 24  # gap between the window and the right-hand labels (more where dog-leg lines run in it)
        self.leaders, self.brackets, self.notes = [], [], []

    def T(self, x, y):
        return (x - self.ox + self.ml, y - self.oy + self.mt)

    def top(self, text, lx, row, target, width=360):
        n = text.count("\n") + 1
        tx, ty = self.T(*target)
        ax = self.T(lx, 0)[0]
        ay = 8 + row * (self.lh * 2.3)
        sy = ay + n * self.lh + 4
        pts = [(ax, sy), (ax, sy + 16), (tx, sy + 16), (tx, ty)] if abs(ax - tx) > 2 else [(ax, sy), (tx, ty)]
        self.leaders.append((text, (ax - width / 2, ay, width, "c"), pts))

    def side(self, left, text, ly, target, width, elbow=None, xe=None):
        """ly: screenshot y of the label's first line centre"""
        tx, ty = self.T(*target)
        n = text.count("\n") + 1
        sy = self.T(0, ly)[1]
        top = sy - self.lh * 0.5
        # the label's first line sits on the line that leaves it
        if left:
            box = (self.ml - 24 - width, top, width, "r")
            x0 = self.ml - 18
        else:
            box = (self.ml + self.iw + self.gapr, top, width, "l")
            x0 = self.ml + self.iw + self.gapr - 6
        ey = self.T(0, elbow)[1] if elbow is not None else None
        if xe is not None:
            # dog-leg: out of the label, along the margin at x = xe px from the window, then in to the target
            ex = (self.ml - xe) if left else (self.ml + self.iw + xe)
            pts = [(x0, sy), (ex, sy), (ex, ty), (tx, ty)]
        elif ey is not None:
            pts = [(x0, sy), (x0 + (10 if left else -10), sy), (x0 + (10 if left else -10), ey), (tx, ey)]
        elif abs(ty - sy) <= 2:
            pts = [(x0, sy), (tx, ty)]
        else:
            pts = [(x0, sy), (tx, sy), (tx, ty)]
        self.leaders.append((text, box, pts))

    def left(self, text, ly, target, width, elbow=None, xe=None):
        self.side(True, text, ly, target, width, elbow, xe)

    def right(self, text, ly, target, width, elbow=None, xe=None):
        self.side(False, text, ly, target, width, elbow, xe)

    def bracket(self, text, x0, x1, y, width=None):
        a, b = self.T(x0, y), self.T(x1, y)
        self.brackets.append((text, a[0], b[0], a[1], width or (b[0] - a[0])))

    def note(self, text, x, y, width):
        px, py = self.T(x, y)
        self.notes.append((text, px, py, width))

    def spec(self):
        return {"image": self.image, "crop": (self.ox, self.oy, self.ox + self.iw, self.oy + self.ih), "at": (self.ml, self.mt),
                "canvas": (self.cw, self.ch), "leaders": self.leaders, "brackets": self.brackets, "notes": self.notes}


# ---- SatComp (screenshot 1904 x 1338; window x 112-1792, y 75-1190)
h = _H("satcomp.png", 250, 330, 230, 330)
h.left("waveshaper\nselector", 307, (146, 307), 200)
h.top("oversample", 747, 0, (747, 303), 220)
h.top("eq page", 1272, 0, (1272, 303), 160)
h.top("enter custom shaper\ndraw mode", 505, 1, (505, 372), 400)
h.top("toggle sidechain input\nfrom channel 3/4", 1040, 1, (1053, 303), 460)
h.right("viewport mode:\nscope / transient", 376, (1352, 376), 300)
h.left("shaper hpf/lpf\npre filter", 900, (146, 935), 250, elbow=935)
h.left("tone is another\nlow-pass filter", 1000, (440, 1005), 250, elbow=1024)
h.right("drag CEIL / FLOOR\nto rescale the plot", 760, (1262, 708), 330)
h.right("drag the curve:\nabove threshold = ratio\nbelow it = threshold", 480, (1250, 462), 330)
h.bracket("waveshaper module", 142, 532, 1215)
h.bracket("compressor module", 568, 1366, 1215)
h.bracket("output module", 1402, 1764, 1215)
h.note("k.shape = knee shape\nff>fb = feedforward / feedback\ngr dist = gain reduction distortion", 568, 1285, 800)
_sc = h.spec()

# ---- SatXComp (1108 x 765; window x 56-1052, y 38-690)
h = _H("satxcomp.png", 250, 280, 40, 250)
h.left("multiband tabs", 399, (74, 399), 220)
h.right("xov = crossover range\n24 dB = slope selector", 250, (1010, 267), 270, elbow=None)
h.right("oversample", 352, (950, 366), 270)
h.right("l.ahead = lookahead", 392, (950, 399), 270)
h.right("compression mode:\ndown, up, down/up", 450, (722, 433), 270)
h.bracket("waveshaper module", 72, 380, 700)
h.bracket("compressor module", 385, 846, 700)
h.bracket("output module", 857, 1040, 700)
h.note("3 compression modes\ndown: normal compression\nup: upward compression\ndown/up: both, OTT style", 385, 770, 560)
_xc = h.spec()

# ---- SatLim (622 x 921; window x 56-566, y 38-845)
h = _H("satlim.png", 260, 270, 30, 140)
h.right("oversample", 384, (480, 384), 250)
h.right("l.ahead = lookahead", 495, (478, 495), 250)
h.left("waveform scope:\ndrag it for the\nwindow and ceiling", 220, (200, 220), 230)
h.left("oversampled shaper\nmeter and clip meter", 560, (292, 600), 230)
h.bracket("waveshaper\nmodule", 72, 266, 852)
h.bracket("limiter / clipper\noutput module", 276, 551, 852)
_sl = h.spec()

# ---- SatDiff (586 x 999; window x 56-530, y 38-925)
h = _H("satdiff.png", 360, 480, 150, 80)
h.gapr = 64
h.left("LMB: cycle color theme\ndouble-LMB: dark / light\nRMB: theme dropdown", 110, (86, 115), 330, elbow=None)
h.top("LMB plugin title:\nabout page", 150, 0, (150, 114), 300)
h.left("scene A / B toggle:\nwaveshaper config tab", 206, (88, 184), 330)
h.left("waveshaper module\nparameters", 575, (74, 580), 330)
h.left("bias tile: no tiling,\nmirror tile, repeat tile", 658, (110, 658), 330)
h.left("color effects module:\ndiffusor, phaser, flanger,\nringmod, frequency shifter", 730, (74, 700), 330)
h.left("output module: hpf mid/side\nfor cleaning up low end\nor a mono sub", 860, (74, 862), 330)
h.top("enter custom shaper\ndraw mode", 395, 1, (395, 222), 300)
h.right("A/B morph module\nLMB cycles the mode, RMB lists it\nXFADE: blends the 2 scenes\nMORPH: a detector drives the blend\n(A on silence, B on loud peaks)", 230, (500, 230), 400)
h.right("depth: interpolation\nintensity", 380, (500, 282), 400, xe=44)
h.right("speed: attack / release\nof the detector", 430, (500, 337), 400, xe=30)
h.right("vertical meter: LMB cycles\nthe timing windows,\nRMB the spectrum", 500, (500, 455), 400, xe=16)
_sd = h.spec()

HEROES = {"satcomp": _sc, "satxcomp": _xc, "satlim": _sl, "satdiff": _sd}
