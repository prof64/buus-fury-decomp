from common import *
import numpy as np
R=np.frombuffer(ROM,dtype=np.uint8)
for size,dt in ((1,np.uint8),(2,np.uint16),(4,np.uint32)):
    for align in range(size):
        a=np.frombuffer(ROM[align:len(ROM)-((len(ROM)-align)%size)],dtype=dt).astype(np.int64)
        for stride in range(1,12):
            m=(a[:-3*stride]==44)
            for k in range(1,4): m&=(a[k*stride:len(a)-(3-k)*stride]==44+2*k)
            for i in np.nonzero(m)[0][:5]:
                print(size,'stride',stride*size,hex(0x08000000+align+i*size))
