"""Original AAC-LC fixtures; FFmpeg is a reference encoder/decoder only."""
from pathlib import Path
import math, subprocess, tempfile, wave

out = Path(__file__).parent
cases = [("mono441",44100,1,10001),("stereo48",48000,2,48013),
         ("stereo96",96000,2,12037),("short48",48000,1,121),("stream48",48000,2,240013)]
with tempfile.TemporaryDirectory() as temporary:
    for name, rate, channels, frames in cases:
        source = Path(temporary)/(name+".wav")
        with wave.open(str(source),"wb") as wav:
            wav.setparams((channels,2,rate,0,"NONE","not compressed"))
            wav.writeframes(b"".join(int(9000*math.sin(math.tau*(997 if c==0 else 2711)*i/rate)).to_bytes(2,"little",signed=True)
                                   for i in range(frames) for c in range(channels)))
        target = out/(name+".m4a")
        subprocess.run(["ffmpeg","-v","error","-nostdin","-y","-i",str(source),"-map_metadata","-1",
                        "-c:a","aac","-b:a","128k","-movie_timescale",str(rate),"-movflags","+faststart",str(target)],check=True)
        subprocess.run(["ffmpeg","-v","error","-nostdin","-y","-i",str(target),"-af",f"atrim=end_sample={frames}",
                        "-f","f32le",str(out/(name+".f32"))],check=True)
    subprocess.run(["ffmpeg","-v","error","-nostdin","-y","-i",str(out/"stereo48.m4a"),"-map_metadata","-1",
                    "-c:a","copy","-movie_timescale","1000","-movflags","+faststart",str(out/"coarse48.m4a")],check=True)
    subprocess.run(["ffmpeg","-v","error","-nostdin","-y","-i",str(out/"mono441.m4a"),"-c:a","copy",
                    "-f","adts",str(out/"mono441.aac")],check=True)
