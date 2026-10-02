"""Reference implementation of LoG2 RNG + damage formula (ROM 0x080213fc, 0x0800f910)."""
M=0xffffffff
class RNG:
    def __init__(s,a,b): s.a=a&M; s.b=b&M
    def next(s):
        r1=((s.a+0x2cbbc71)&M)^s.b
        r0=((((s.b<<3)|(s.b>>29))&M)+0x632d80f)&M
        r0^=r1
        s.b=r0; s.a=r1
        return (r1-r0)&M
    def rand(s,n): return (((s.next()&0xffff)*n)&M)>>16
    def chance(s,p): return 1 if (s.next()&0xffff)<p else 0
def asr(x,n): return x>>n
def stat_curve(S):
    return (((26*S-701)*S+115618)*S)>>8
def damage(atype, power, S, d_def, d_mult, scale, rng):
    """returns (raw_before_mult, final, crit)"""
    crit=0
    if atype==0:
        crit=rng.chance((0xa4*S)&M)
        power=power*(crit+1)
    b2=(power*(0x80-d_def))>>7
    f=stat_curve(S)
    r=rng.rand(0x1a)+256
    t=((r*b2)&M)>>8
    t=((t*f)&M)>>18
    t=(scale*t)>>10
    return t,(t*d_mult)>>7,crit
