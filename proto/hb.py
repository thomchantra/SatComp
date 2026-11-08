"""Generic halfband stage (N=2K+1) mirroring the JSFX ring-buffer logic; latency + aliasing for candidate K pairs."""
import numpy as np
def taps(K, beta):
    N = 2*K+1; n = np.arange(N)-K
    h = np.sinc(n/2)*.5*np.kaiser(N, beta); h[K] = .5
    odd = np.array([h[2*j+1] for j in range(K)]); odd *= .5/odd.sum()
    return odd
def pow2(n): return 1 << (n-1).bit_length()
class Stage:
    def __init__(s, K, beta):
        s.K=K; s.t=taps(K,beta); s.um=pow2(K)-1; s.dm=pow2(2*K)-1
        s.u=[0.0]*(s.um+1); s.d=[0.0]*(s.dm+1); s.up_=0; s.dq=0
    def up(s,x):
        p=s.up_; s.u[p]=x; um=s.um
        y0=s.u[(p-s.K//2) & um]
        acc=sum(s.t[j]*s.u[(p-j)&um] for j in range(s.K))
        s.up_=(p+1)&um; return y0,2*acc
    def down(s,a,b):
        q=s.dq; dm=s.dm; s.d[q]=a
        acc=.5*s.d[(q-s.K)&dm]+sum(s.t[j]*s.d[(q-(2*j+1))&dm] for j in range(s.K))
        q=(q+1)&dm; s.d[q]=b; s.dq=(q+1)&dm; return acc
def run(x, KA, KB, mode, f=lambda v:v, beta=(10,10)):
    A=Stage(KA,beta[0]); B=Stage(KB,beta[1]); out=[]
    for v in x:
        a0,a1=A.up(v)
        if mode==1: out.append(A.down(f(a0),f(a1)))
        else:
            b0,b1=B.up(a0); b2,b3=B.up(a1)
            out.append(A.down(B.down(f(b0),f(b1)),B.down(f(b2),f(b3))))
    return np.array(out)
def alias(f0, KA, KB, mode, beta):
    t=np.arange(48000); tone=np.sin(2*np.pi*t*f0/48000)*4
    ya=np.tanh(tone) if mode==0 else run(tone,KA,KB,mode,np.tanh,beta)
    S=np.abs(np.fft.rfft(ya[3000:3000+32768]*np.hanning(32768))); fr=np.fft.rfftfreq(32768,1/48000)
    harm=np.zeros_like(S,bool)
    for hh in range(1,12): harm|=np.abs(fr-f0*hh)<40
    return 20*np.log10(S[~harm].max()/S.max())
if __name__=="__main__":
    for KA,KB,beta in ((16,16,(8,8)),(24,8,(10,8)),(32,8,(10,8)),(24,8,(8,6))):
        lat1=KA; lat2=KA+2*KB//4
        t=np.arange(3000); tone=np.sin(2*np.pi*t*3000/48000)
        e1=np.abs(run(tone,KA,KB,1,beta=beta)[lat1+100:]-tone[100:-lat1]).max()
        e2=np.abs(run(tone,KA,KB,2,beta=beta)[lat2+100:]-tone[100:-lat2]).max()
        print(f"KA={KA} KB={KB} beta={beta}: lat x2={lat1} x4={lat2} nullerr {e1:.1e}/{e2:.1e}  alias2.3k off/x2/x4 = "
              f"{alias(2300,KA,KB,0,beta):.0f}/{alias(2300,KA,KB,1,beta):.0f}/{alias(2300,KA,KB,2,beta):.0f}  5k = "
              f"{alias(5000,KA,KB,0,beta):.0f}/{alias(5000,KA,KB,1,beta):.0f}/{alias(5000,KA,KB,2,beta):.0f}")
