#include <math.h>
#include <stdio.h>
#include <stdlib.h>
#include <vorbis/vorbisenc.h>

static void write_page(FILE *file, ogg_page *page) {
    if (fwrite(page->header, 1, page->header_len, file) != (size_t)page->header_len ||
        fwrite(page->body, 1, page->body_len, file) != (size_t)page->body_len) exit(3);
}

int main(int argc, char **argv) {
    if (argc != 6) return 2;
    int channels = atoi(argv[2]);
    long long frames = atoll(argv[3]);
    long long offset = atoll(argv[4]);
    int serial = atoi(argv[5]);
    FILE *file = fopen(argv[1], "wb");
    if (!file || (channels != 1 && channels != 2) || frames < 1) return 2;
    vorbis_info info;
    vorbis_comment comment;
    vorbis_dsp_state dsp;
    vorbis_block block;
    ogg_stream_state stream;
    ogg_packet packet, header, comments, setup;
    ogg_page page;
    vorbis_info_init(&info);
    if (vorbis_encode_init_vbr(&info, channels, 48000, 0.3f)) return 4;
    vorbis_comment_init(&comment);
    vorbis_comment_add_tag(&comment, "ENCODER", "Aede synthetic fixture / libvorbis 1.3.7");
    if (vorbis_analysis_init(&dsp, &info) || vorbis_block_init(&dsp, &block) ||
        ogg_stream_init(&stream, serial)) return 4;
    vorbis_analysis_headerout(&dsp, &comment, &header, &comments, &setup);
    ogg_stream_packetin(&stream, &header);
    ogg_stream_packetin(&stream, &comments);
    ogg_stream_packetin(&stream, &setup);
    while (ogg_stream_flush(&stream, &page)) write_page(file, &page);
    long long written = 0;
    int ended = 0;
    while (!ended) {
        int count = (int)((frames - written) < 1024 ? (frames - written) : 1024);
        float **pcm = vorbis_analysis_buffer(&dsp, count ? count : 1);
        for (int i = 0; i < count; ++i) {
            double t = (double)(offset + written + i) / 48000.0;
            for (int channel = 0; channel < channels; ++channel) {
                double phase = channel * 0.43 + 0.17;
                pcm[channel][i] = (float)(0.16 * sin(6.283185307179586 * 997.0 * t + phase) +
                                        0.06 * sin(6.283185307179586 * 5503.0 * t + phase * 0.7));
            }
        }
        vorbis_analysis_wrote(&dsp, count);
        written += count;
        if (!count) ended = 1;
        while (vorbis_analysis_blockout(&dsp, &block) == 1) {
            vorbis_analysis(&block, NULL);
            vorbis_bitrate_addblock(&block);
            while (vorbis_bitrate_flushpacket(&dsp, &packet)) {
                ogg_stream_packetin(&stream, &packet);
                while (ogg_stream_pageout(&stream, &page)) write_page(file, &page);
            }
        }
    }
    while (ogg_stream_flush(&stream, &page)) write_page(file, &page);
    ogg_stream_clear(&stream);
    vorbis_block_clear(&block);
    vorbis_dsp_clear(&dsp);
    vorbis_comment_clear(&comment);
    vorbis_info_clear(&info);
    if (fclose(file)) return 3;
    fprintf(stderr, "%s: %lld input frames, %d channels, offset %lld; %s\n",
            argv[1], frames, channels, offset, vorbis_version_string());
    return 0;
}
