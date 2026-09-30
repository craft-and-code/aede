#include <stdio.h>
#include <vorbis/vorbisfile.h>

int main(int argc, char **argv) {
    if (argc != 3) return 2;
    OggVorbis_File file;
    if (ov_fopen(argv[1], &file)) return 3;
    vorbis_info *info = ov_info(&file, -1);
    FILE *output = fopen(argv[2], "wb");
    if (!output) return 3;
    float **pcm;
    long count;
    long long frames = 0;
    int link;
    while ((count = ov_read_float(&file, &pcm, 4096, &link)) > 0) {
        for (long frame = 0; frame < count; ++frame) {
            for (int channel = 0; channel < info->channels; ++channel) {
                if (fwrite(&pcm[channel][frame], sizeof(float), 1, output) != 1) return 4;
            }
        }
        frames += count;
    }
    printf("%s rate=%ld channels=%d granule_frames=%lld decoded_frames=%lld status=%ld\n",
           argv[1], info->rate, info->channels, (long long)ov_pcm_total(&file, -1), frames, count);
    ov_clear(&file);
    fclose(output);
    return count < 0 ? 5 : 0;
}
