#include "bridge.h"
#include <wavpack.h>
#include <stdio.h>

static int32_t read_bytes(void *p, void *b, int32_t n) {
    AedeReader *r = p; return r->read(r->user, b, n);
}
static int64_t tell(void *p) { AedeReader *r=p; return r->tell(r->user); }
static int seek_abs(void *p, int64_t n) {
    AedeReader *r=p; return r->seek(r->user,n) ? 0 : -1;
}
static int seek_rel(void *p, int64_t n, int mode) {
    AedeReader *r=p;
    int64_t base = mode==SEEK_SET ? 0 : mode==SEEK_CUR ? tell(p) :
                   mode==SEEK_END ? r->length : -1;
    if (base<0 || (n>0 && base>INT64_MAX-n) || (n<0 && n < -base)) return -1;
    return seek_abs(p,base+n);
}
static int push(void *p,int ch) { return seek_rel(p,-1,SEEK_CUR)==0 ? ch : EOF; }
static int64_t length(void *p) { return ((AedeReader*)p)->length; }
static int seekable(void *p) { (void)p; return 1; }
static WavpackStreamReader64 reader = {
    read_bytes,0,tell,seek_abs,seek_rel,push,length,seekable,0,0
};
void *aede_wv_open(AedeReader *r,AedeInfo *info) {
    char error[80] = {0};
    /* Adapt floating-point source exponents to the shared +/-1 PCM domain.
       This is format scaling, without playback gain or loudness processing.
       No filesystem names, correction-file discovery or codec threads. */
    WavpackContext *w = WavpackOpenFileInputEx64(&reader,r,0,error,OPEN_NORMALIZE,0);
    if (!w) return 0;
    info->rate=WavpackGetSampleRate(w);
    info->channels=WavpackGetNumChannels(w);
    info->bits=WavpackGetBitsPerSample(w);
    info->floating=(WavpackGetMode(w)&MODE_FLOAT)!=0;
    info->lossless=(WavpackGetMode(w)&MODE_LOSSLESS)!=0;
    info->frames=WavpackGetNumSamples64(w);
    return w;
}
int aede_wv_decode(void *p,int32_t *out,uint32_t frames) {
    uint32_t n=WavpackUnpackSamples(p,out,frames);
    return WavpackGetNumErrors(p) ? -1 : (int)n;
}
void aede_wv_close(void *p) { WavpackCloseFile(p); }
