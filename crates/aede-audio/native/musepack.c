#include "bridge.h"
#include <mpc/mpcdec.h>
#include <stdlib.h>
typedef struct { mpc_reader reader; AedeReader *source; mpc_demux *demux; } Context;
static mpc_int32_t read_bytes(mpc_reader *p,void *b,mpc_int32_t n) {
    Context *c=p->data; return c->source->read(c->source->user,b,n);
}
static mpc_bool_t seek(mpc_reader *p,mpc_int32_t n) {
    Context *c=p->data; return c->source->seek(c->source->user,n)!=0;
}
static mpc_int32_t tell(mpc_reader *p) {
    Context *c=p->data; return (mpc_int32_t)c->source->tell(c->source->user);
}
static mpc_int32_t length(mpc_reader *p) {
    return (mpc_int32_t)((Context*)p->data)->source->length;
}
static mpc_bool_t seekable(mpc_reader *p) { (void)p; return 1; }
void *aede_mpc_open(AedeReader *source,AedeInfo *out) {
    if (source->length>INT32_MAX) return 0;
    Context *c=calloc(1,sizeof(*c)); if (!c) return 0;
    c->source=source;
    c->reader=(mpc_reader){read_bytes,seek,tell,length,seekable,c};
    c->demux=mpc_demux_init(&c->reader);
    if (!c->demux) { free(c); return 0; }
    mpc_streaminfo info; mpc_demux_get_info(c->demux,&info);
    out->rate=info.sample_freq; out->channels=info.channels;
    out->bits=0; out->floating=1; out->lossless=0;
    out->frames=mpc_streaminfo_get_length_samples(&info);
    return c;
}
int aede_mpc_decode(void *p,float *out,uint32_t capacity) {
    Context *c=p;
    if (capacity<MPC_DECODER_BUFFER_LENGTH) return -1;
    mpc_frame_info frame={0}; frame.buffer=out;
    if (mpc_demux_decode(c->demux,&frame)!=MPC_STATUS_OK) return -1;
    if (frame.bits==-1) return 0;
    if (frame.samples==0) return -2;
    return (int)frame.samples;
}
void aede_mpc_close(void *p) {
    Context *c=p; mpc_demux_exit(c->demux); free(c);
}
