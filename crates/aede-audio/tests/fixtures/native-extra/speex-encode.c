/* Reference fixture encoder only; never compiled or used by product code. */
#include <speex/speex.h>
#include <speex/speex_stereo.h>
#include <stdio.h>
#include <stdlib.h>
#include <stdint.h>
int main(int argc,char **argv) {
    if(argc!=7)return 2;
    int mode=atoi(argv[1]),channels=atoi(argv[2]),per_packet=atoi(argv[3]);
    if(mode<0||mode>2||channels<1||channels>2||per_packet<1||per_packet>16)return 2;
    FILE *input=fopen(argv[4],"rb"),*output=fopen(argv[5],"wb");
    if(!input||!output)return 3;
    void *state=speex_encoder_init(speex_lib_get_mode(mode));if(!state)return 3;
    int size=0,lookahead=0,quality=8,rate=8000*(1<<mode);
    speex_encoder_ctl(state,SPEEX_SET_QUALITY,&quality);
    speex_encoder_ctl(state,SPEEX_SET_SAMPLING_RATE,&rate);
    speex_encoder_ctl(state,SPEEX_GET_FRAME_SIZE,&size);
    speex_encoder_ctl(state,SPEEX_GET_LOOKAHEAD,&lookahead);
    SpeexBits bits;speex_bits_init(&bits);
    short samples[1280];char packet[65536];int finished=0,packets=0;
    while(!finished) {
        speex_bits_reset(&bits);
        for(int n=0;n<per_packet;n++) {
            size_t count=fread(samples,sizeof(short),size*channels,input);
            if(count<(size_t)(size*channels)) {
                for(size_t j=count;j<(size_t)(size*channels);j++)samples[j]=0;
                finished=1;
            }
            if(channels==2)speex_encode_stereo_int(samples,size,&bits);
            if(speex_encode_int(state,samples,&bits)<0)return 4;
        }
        speex_bits_insert_terminator(&bits);
        int bytes=speex_bits_write(&bits,packet,sizeof(packet));
        unsigned char length[4]={(unsigned char)bytes,(unsigned char)(bytes>>8),(unsigned char)(bytes>>16),(unsigned char)(bytes>>24)};
        fwrite(length,1,4,output);fwrite(packet,1,bytes,output);packets++;
    }
    FILE *info=fopen(argv[6],"w");if(!info)return 3;
    fprintf(info,"%d %d %d %d\n",size,lookahead,packets,speex_lib_get_mode(mode)->bitstream_version);
    fclose(info);fclose(input);fclose(output);speex_bits_destroy(&bits);speex_encoder_destroy(state);return 0;
}
