"""Original codec fixtures. Tools here are references, never product dependencies."""
from pathlib import Path
import math, struct, subprocess, tempfile, wave, sys
out=Path(__file__).parent

def crc(data):
    value=0
    for byte in data:
        value^=byte<<24
        for _ in range(8):value=((value<<1)^ (0x04c11db7 if value&0x80000000 else 0))&0xffffffff
    return value

def page(packet,serial,sequence,flags,granule):
    laces=bytes([255]*(len(packet)//255)+[len(packet)%255])
    header=bytearray(b'OggS'+bytes([0,flags])+struct.pack('<QII',granule,serial,sequence)+b'\0'*4+bytes([len(laces)]))
    raw=header+laces+packet;raw[22:26]=struct.pack('<I',crc(raw));return raw

def reference_speex(source,target,temp):
    raw=temp/'reference-speex.raw'
    subprocess.run([sys.argv[3],str(source),str(raw)],check=True,stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL)
    samples=(v[0]/32768 for v in struct.iter_unpack('<h',raw.read_bytes()))
    target.write_bytes(b''.join(struct.pack('<f',v) for v in samples))

if len(sys.argv)!=4:raise SystemExit('Usage: generate.py <reference mpcenc> <fixture speex-encode> <reference speexdec>')
with tempfile.TemporaryDirectory() as d:
    temp=Path(d)
    for name,mode,channels,fpp,frames in [('speex-nb',0,1,1,8013),('speex-wb',1,2,2,16013),('speex-uwb',2,2,1,32013)]:
        rate=8000*(1<<mode)
        raw=b''.join(int(10000*math.sin(math.tau*(997 if c==0 else 2711)*i/rate)).to_bytes(2,'little',signed=True) for i in range(frames) for c in range(channels))
        (temp/'input.raw').write_bytes(raw)
        subprocess.run([sys.argv[2],str(mode),str(channels),str(fpp),str(temp/'input.raw'),str(temp/'packets'),str(temp/'info')],check=True)
        size,lookahead,count,version=map(int,(temp/'info').read_text().split())
        header=b'Speex   '+b'Aede fixture encoder'.ljust(20,b'\0')+struct.pack('<13i',1,80,rate,mode,version,channels,-1,size,0,fpp,0,0,0)
        comment=struct.pack('<I',4)+b'Aede'+struct.pack('<I',0)
        stream=bytearray(page(header,0xA3DE,0,2,0)+page(comment,0xA3DE,1,0,0))
        packets=(temp/'packets').read_bytes();at=0
        for index in range(count):
            length=struct.unpack_from('<I',packets,at)[0];at+=4;data=packets[at:at+length];at+=length
            stream.extend(page(data,0xA3DE,index+2,4 if index+1==count else 0,(index+1)*size*fpp))
        target=out/(name+'.spx');target.write_bytes(stream)
        reference_speex(target,out/(name+'.f32'),temp)
    source=temp/'tone.wav'
    with wave.open(str(source),'wb') as wav:
        wav.setparams((2,2,44100,0,'NONE','not compressed'))
        wav.writeframes(b''.join(int(10000*math.sin(math.tau*(997 if c==0 else 2711)*i/44100)).to_bytes(2,'little',signed=True) for i in range(44113) for c in range(2)))
    target=out/'tone-sv8.mpc'
    subprocess.run([sys.argv[1],'--silent','--overwrite',str(source),str(target)],check=True,stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL)
    # FFmpeg's Musepack reader retains the public 481-frame synthesis pre-roll.
    subprocess.run(['ffmpeg','-v','error','-nostdin','-y','-i',str(target),'-af','atrim=start_sample=481:end_sample=44594','-f','f32le',str(out/'tone-sv8.f32')],check=True)
    reference_speex(out/'track.spx',out/'track.spx.f32',temp)
    for name in ['track.wv','float32.wv','hybrid-lossy.wv','ramp.ape']:
        subprocess.run(['ffmpeg','-v','error','-nostdin','-y','-i',str(out/name),'-f','f32le',str(out/(name+'.f32'))],check=True)
