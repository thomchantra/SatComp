"""Numpy prototypes mirroring the SatComp.jsfx math: knee C1 check, shaper normalisation, halfband OS latency/null."""
import numpy as np

# ---- Knee family -----------------------------------------------------------
def knee_gr(x, T, W, R, shape):
    s = 1.0 - (0.0 if np.isinf(R) else 1.0 / R)
    sh2 = shape * 2
    c0, c1, c2 = (1 - sh2, sh2, 0) if sh2 <= 1 else (0, 2 - sh2, sh2 - 1)
    d = x - T
    out = np.zeros_like(x)
    hi = d >= W / 2
    out[hi] = d[hi] * s
    m = (d > -W / 2) & ~hi
    u = (d[m] + W / 2) / W
    G = c0*u**2/2 + c1*(u**3 - u**4/2) + c2*(u**6 - 3*u**5 + 2.5*u**4)
    out[m] = s * W * G
    return out

x = np.linspace(-60, 0, 600001)
for R in (2, 4, 20, np.inf):
    for sh in (0, .25, .5, .75, 1):
        gr = knee_gr(x, -20, 12, R, sh)
        d1 = np.diff(gr) / np.diff(x)
        d2 = np.abs(np.diff(d1)).max()
        assert np.abs(np.diff(gr)).max() < 1e-3, "value jump"
        assert d2 < 1e-3, ("slope jump", R, sh, d2)
        assert (gr >= -1e-12).all() and (d1 >= -1e-9).all(), "GR must be monotone"
print("knee: C1 + monotone for all shapes/ratios OK")

# ---- Shapers -----------------------------------------------------------------
def tri(x): return (2/np.pi)*np.arcsin(np.sin(np.pi*x/2))
def hard(x, a=.9):
    ax = np.abs(x); return np.where(ax <= a, x, np.sign(x)*(a + (1-a)*np.tanh((ax-a)/(1-a))))
def cheby(x):
    t = np.tanh(x); return 1.6*t + .3*2*t**2 + .2*(4*t**3 - 3*t)
SH = {
 'Tube': lambda x: np.where(x >= 0, np.tanh(x), np.tanh(.6*x)/.6),
 'Soft': np.tanh,
 'Rational': lambda x: x/(1+1.5*np.abs(x)),
 'Atan': lambda x: (2/np.pi)*np.arctan(np.pi/2*x),
 'Hard': hard, 'TriFold': tri,
 'SineFold': lambda x: np.sin(np.pi/2*x)/(np.pi/2),
 'Crossover': lambda x: x - .12*np.tanh(x/.15),
 'Cheby': cheby,
}
e = 1e-5
for k, f in SH.items():
    slope = (f(np.array([e]))-f(np.array([-e])))/(2*e)
    xs = np.linspace(-8, 8, 4097); y = f(xs)
    sym = np.abs(y + y[::-1]).max()
    print(f"{k:10s} slope0={slope[0]:.3f} peak={np.abs(y).max():.3f} odd-asym={sym:.3f}")

# ---- Halfband stage: mirror of hb_up / hb_down ring logic ---------------------
N = 33
n = np.arange(N) - 16
h = np.where(n == 0, .5, np.sinc(n/2)*.5) * np.kaiser(N, 8.0)
h[16] = .5
odd = np.array([h[2*j+1] for j in range(16)])
odd *= .5/odd.sum()          # DC normalise (sum of odd taps == 0.5)

class Stage:
    def __init__(s): s.u = [0.0]*16; s.upos = 0; s.d = [0.0]*32; s.dq = 0
    def up(s, x):
        p = s.upos; s.u[p] = x
        y0 = s.u[(p + 8) & 15]
        acc = sum(odd[j]*s.u[(p - j + 16) & 15] for j in range(16))
        s.upos = (p+1) & 15
        return y0, 2*acc
    def down(s, a, b):
        q = s.dq; s.d[q] = a
        acc = .5*s.d[(q+16) & 31] + sum(odd[j]*s.d[(q-(2*j+1)+32) & 31] for j in range(16))
        q = (q+1) & 31; s.d[q] = b; s.dq = (q+1) & 31
        return acc

def run(x, mode, f=lambda v: v):
    A, B = Stage(), Stage(); out = []
    for v in x:
        a0, a1 = A.up(v)
        if mode == 1:
            out.append(A.down(f(a0), f(a1)))
        else:
            b0, b1 = B.up(a0); b2, b3 = B.up(a1)
            c0 = B.down(f(b0), f(b1)); c1 = B.down(f(b2), f(b3))
            out.append(A.down(c0, c1))
    return np.array(out)

rng = np.random.default_rng(1)
sig = rng.standard_normal(2000)
for mode, lat in ((1, 16), (2, 24)):
    y = run(sig, mode)
    err = np.abs(y[lat:] - sig[:-lat])
    # band-limited noise would be exact; white noise has HF loss, so test a 3 kHz-ish tone at fs=48k
    t = np.arange(3000); tone = np.sin(2*np.pi*t*3000/48000)
    yt = run(tone, mode)
    print(f"OS x{mode*2 if mode==1 else 4}: latency {lat}, tone null err {np.abs(yt[lat+100:]-tone[100:-lat]).max():.2e}")
    # aliasing: tanh-ish hard drive of 9 kHz tone @48k, measure alias at fold products
    t = np.arange(48000); tone = np.sin(2*np.pi*t*9000/48000)*3
    for name, mm in (('off', 0), ('on', mode)):
        ya = np.tanh(tone) if mm == 0 else run(tone, mm, np.tanh)
        S = np.abs(np.fft.rfft(ya[2000:2000+32768]*np.hanning(32768)))
        fr = np.fft.rfftfreq(32768, 1/48000)
        harm = np.zeros_like(S, bool)
        for hh in range(1, 10): 
            harm |= np.abs(fr - 9000*hh) < 60
        # nonharmonic energy (alias) relative to fundamental
        print(f"   aliasing {name}: {20*np.log10(S[~harm].max()/S.max()):.1f} dB")

print("--- aliasing, 2.3 kHz tone, tanh drive ---")
for f0 in (2300, 5000):
    t = np.arange(48000); tone = np.sin(2*np.pi*t*f0/48000)*4
    for name, mm in (('off', 0), ('x2', 1), ('x4', 2)):
        ya = np.tanh(tone) if mm == 0 else run(tone, mm, np.tanh)
        S = np.abs(np.fft.rfft(ya[2000:2000+32768]*np.hanning(32768)))
        fr = np.fft.rfftfreq(32768, 1/48000)
        harm = np.zeros_like(S, bool)
        for hh in range(1, 12): harm |= np.abs(fr - f0*hh) < 40
        print(f"  {f0} Hz {name}: worst alias {20*np.log10(S[~harm].max()/S.max()):.1f} dB")
