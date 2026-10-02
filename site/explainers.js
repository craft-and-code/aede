/* Pedagogical models only: these plots never play or process visitor audio. */
(() => {
  "use strict";
  const french = document.documentElement.lang === "fr";
  const words = french ? {
    pipeline:"Du fichier à la sortie", continuity:"Deux pistes, une session ou une remise à zéro", measurements:"Crêtes entre les échantillons", normalization:"Un niveau cible, une réserve pour les crêtes", tone:"Deux corrections larges, une réserve commune", channels:"Du 5.1 à la stéréo, sans deviner les canaux", resampling:"Changer de fréquence, préserver la bande utile", dither:"Quantification et bruit triangulaire", metering:"Observer les crêtes et le garde-fou", spectrum:"24 bandes pour lire le spectre",
    intro:"Faites varier les paramètres pour comprendre le mécanisme. Le signal et les courbes sont synthétiques.", source:"Entrée", output:"Sortie", guard:"Limite", disclaimer:"Illustration pédagogique simplifiée. Aucun son n’est joué et aucun fichier n’est modifié.", phase:"Position des échantillons", cycles:"Oscillations", reconstructed:"Signal continu du modèle", samplePeak:"Crête d’échantillon", truePeak:"Crête continue du modèle", gain:"Gain illustré", level:"Niveau source", target:"Cible", peak:"Crête source", bass:"Graves", treble:"Aigus", frequency:"Fréquence du signal", rate:"Fréquence de sortie", bits:"Bits pour l’illustration", ditherLabel:"Dither TPDF", enabled:"Activé", disabled:"Désactivé", channel:"Canal source isolé", amplitude:"Amplitude du signal", stage:"Étape observée", reserve:"Réserve", limited:"Le gain est plafonné par la crête", available:"La cible reste accessible", omitted:"LFE omis du mélange stéréo", coefficients:"Coefficients normalisés pour préserver les crêtes", filtered:"Au-dessus de la bande utile : composante filtrée", preserved:"Dans la bande utile : composante conservée", error:"Erreur de quantification", more:"Le bruit rend l’erreur moins corrélée au signal, sans augmenter la résolution", clipping:"Le garde-fou écrête les échantillons au-delà de ±1", clean:"Les échantillons restent dans ±1", noLimiter:"Le garde-fou n’est pas un limiteur de true peak", sample:"Échantillons", decode:"Décodage", stereo:"Canaux", outputGuard:"Garde-fou", transition:"Transition", natural:"Jonction naturelle", skip:"Saut de piste", compatible:"Formats compatibles", different:"Formats différents", format:"Compatibilité", trim:"Remplissage encodeur retiré", trackA:"Piste A", trackB:"Piste B", session:"État DSP conservé", reset:"État DSP réinitialisé", hardware:"Le schéma ne garantit pas une jonction sans coupure sur le périphérique réel", normalize:"Gain", eq:"Tonalité", convert:"Fréquence", meter:"Mesures", device:"Sortie", hz:"Hz", noteBits:"La résolution réduite rend l’effet visible. Les sorties entières réelles utilisent les formats négociés avec le périphérique.", time:"Temps", db:"dB", bands:"Bandes de fréquence logarithmiques"
  } : {
    pipeline:"From file to output", continuity:"Two tracks, one session or a reset", measurements:"Peaks between samples", normalization:"A target level and reserve for peaks", tone:"Two broad controls, one shared reserve", channels:"From 5.1 to stereo, without guessing channels", resampling:"Change the rate, preserve the useful band", dither:"Quantisation and triangular noise", metering:"Observe peaks and the output guard", spectrum:"24 bands to read the spectrum",
    intro:"Adjust the parameters to understand the mechanism. The signal and curves are synthetic.", source:"Input", output:"Output", guard:"Limit", disclaimer:"Simplified educational illustration. No audio is played and no file is modified.", phase:"Sample positions", cycles:"Oscillations", reconstructed:"Model continuous signal", samplePeak:"Sample peak", truePeak:"Model continuous peak", gain:"Illustrated gain", level:"Source level", target:"Target", peak:"Source peak", bass:"Bass", treble:"Treble", frequency:"Signal frequency", rate:"Output rate", bits:"Bits for illustration", ditherLabel:"TPDF dither", enabled:"Enabled", disabled:"Disabled", channel:"Isolated source channel", amplitude:"Signal amplitude", stage:"Observed stage", reserve:"Reserve", limited:"Peak headroom caps the gain", available:"The target remains reachable", omitted:"LFE omitted from the stereo mix", coefficients:"Normalised coefficients preserve peak headroom", filtered:"Above the useful band: component filtered", preserved:"Inside the useful band: component retained", error:"Quantisation error", more:"Noise makes the error less correlated with the signal, without adding resolution", clipping:"The output guard clips samples beyond ±1", clean:"Samples remain inside ±1", noLimiter:"The guard is not a true-peak limiter", sample:"Samples", decode:"Decode", stereo:"Channels", outputGuard:"Guard", transition:"Transition", natural:"Natural join", skip:"Track skip", compatible:"Compatible formats", different:"Different formats", format:"Compatibility", trim:"Encoder padding removed", trackA:"Track A", trackB:"Track B", session:"DSP state retained", reset:"DSP state reset", hardware:"This diagram does not guarantee gapless joins on real hardware", normalize:"Gain", eq:"Tone", convert:"Rate", meter:"Meters", device:"Output", hz:"Hz", noteBits:"Reduced resolution makes the effect visible. Real integer outputs use the formats negotiated with the device.", time:"Time", db:"dB", bands:"Logarithmic frequency bands"
  };
  const clamp = (value, low, high) => Math.min(high, Math.max(low, value));
  const db = value => 20 * Math.log10(Math.max(1e-12, Math.abs(value)));
  const amplitude = value => 10 ** (value / 20);
  const safeGain = (sourceLufs, targetLufs, peakDb) => Math.min(targetLufs - sourceLufs, -peakDb);
  const reserve = (bass, treble) => Math.max(0, bass) + Math.max(0, treble);
  const downmix = channel => {
    const sum = 1 + 2 / Math.sqrt(2);
    return { FL:[1 / sum, 0], FR:[0, 1 / sum], FC:[1 / Math.sqrt(2) / sum, 1 / Math.sqrt(2) / sum], LFE:[0, 0], SL:[1 / Math.sqrt(2) / sum, 0], SR:[0, 1 / Math.sqrt(2) / sum] }[channel];
  };
  // RBJ S=1 shelf magnitude at 48 kHz, the same broad shelf shape used by
  // the documented normal-rate controls. State/transients are not simulated.
  const shelf = (frequency, corner, gain, high) => {
    if (gain === 0) return 0;
    const A = 10 ** (gain / 40), w = 2 * Math.PI * corner / 48000, c = Math.cos(w), alpha = Math.sin(w) / Math.sqrt(2), r = 2 * Math.sqrt(A) * alpha;
    const b = high ? [A*((A+1)+(A-1)*c+r), -2*A*((A-1)+(A+1)*c), A*((A+1)+(A-1)*c-r)] : [A*((A+1)-(A-1)*c+r), 2*A*((A-1)-(A+1)*c), A*((A+1)-(A-1)*c-r)];
    const a = high ? [(A+1)-(A-1)*c+r, 2*((A-1)-(A+1)*c), (A+1)-(A-1)*c-r] : [(A+1)+(A-1)*c+r, -2*((A-1)+(A+1)*c), (A+1)+(A-1)*c-r];
    const omega = 2 * Math.PI * frequency / 48000;
    const magnitude = coefficients => Math.hypot(coefficients[0] + coefficients[1] * Math.cos(omega) + coefficients[2] * Math.cos(2*omega), -coefficients[1]*Math.sin(omega)-coefficients[2]*Math.sin(2*omega));
    return db(magnitude(b) / magnitude(a));
  };
  const toneResponse = (frequency, bass, treble) => shelf(frequency,120,bass,false) + shelf(frequency,4000,treble,true) - reserve(bass,treble);
  const harmonicSine = Math.sqrt(1.66 / 2.64);
  const harmonicPeak = harmonicSine * (1.66 - .88 * harmonicSine ** 2);
  const sampleSignal = (x, phase=0) => (Math.sin(2*Math.PI*(3*x+phase))+.22*Math.sin(2*Math.PI*(9*x+3*phase)))/harmonicPeak;
  // Export the small numerical contracts for offline verification. The UI
  // keeps no visitor data, and there is no network or audio API here.
  window.AedeDspModels = Object.freeze({ safeGain, reserve, downmix, toneResponse, clamp, sampleSignal });
  const specifications = {
    pipeline:[{key:"stage",label:words.stage,options:[words.decode,words.stereo,words.convert,words.normalize,words.eq,words.outputGuard,words.meter,words.device],value:"3"},{key:"gain",label:words.gain,min:-12,max:6,step:1,value:0,unit:"dB"}],
    continuity:[{key:"transition",label:words.transition,options:[words.natural,words.skip],value:"0"},{key:"format",label:words.format,options:[words.compatible,words.different],value:"0"}],
    measurements:[{key:"phase",label:words.phase,min:0,max:1,step:.005,value:.125,unit:""},{key:"cycles",label:words.cycles,min:1,max:8,step:1,value:6,unit:""}],
    normalization:[{key:"level",label:words.level,min:-30,max:-8,step:1,value:-24,unit:"LUFS"},{key:"target",label:words.target,min:-24,max:-14,step:1,value:-18,unit:"LUFS"},{key:"peak",label:words.peak,min:-12,max:0,step:.5,value:-3,unit:"dBTP"}],
    tone:[{key:"bass",label:words.bass,min:-12,max:12,step:1,value:4,unit:"dB"},{key:"treble",label:words.treble,min:-12,max:12,step:1,value:2,unit:"dB"}],
    channels:[{key:"channel",label:words.channel,options:["FL","FR","FC","LFE","SL","SR"],value:"2"}],
    resampling:[{key:"frequency",label:words.frequency,min:500,max:28000,step:500,value:8000,unit:"Hz"},{key:"rate",label:words.rate,options:["44100","48000","96000"],value:"1"}],
    dither:[{key:"bits",label:words.bits,min:3,max:8,step:1,value:5,unit:"bits"},{key:"dither",label:words.ditherLabel,options:[words.disabled,words.enabled],value:"1"}],
    metering:[{key:"amplitude",label:words.amplitude,min:.2,max:1.6,step:.05,value:1.2,unit:""}],
    spectrum:[{key:"frequency",label:words.frequency,min:40,max:12000,step:40,value:440,unit:"Hz"}],
  };
  const reduced = window.matchMedia("(prefers-reduced-motion: reduce)");
  for (const container of document.querySelectorAll("[data-explainer]")) {
    const requested = container.dataset.explainer;
    const kind = requested === "metering" && location.pathname.includes("spectrum") ? "spectrum" : requested === "metering" && location.pathname.includes("measurements") ? "measurements" : requested;
    if (!specifications[kind]) continue;
    const title = document.createElement("h2"); title.textContent = words[kind];
    const intro = document.createElement("p"); intro.textContent = words.intro;
    const chart = document.createElement("div"); chart.className = "dsp-chart";
    const canvas = document.createElement("canvas"); canvas.setAttribute("role", "img"); canvas.setAttribute("aria-label", words[kind]); chart.append(canvas);
    const controls = document.createElement("div"); controls.className = "dsp-controls";
    const values = {}, outputs = {};
    for (const specification of specifications[kind]) {
      const label = document.createElement("label"); label.append(document.createTextNode(specification.label));
      const control = document.createElement(specification.options ? "select" : "input");
      if (specification.options) {
        specification.options.forEach((item, index) => { const option = document.createElement("option"); option.value = String(index); option.textContent = item; control.append(option); });
      } else {
        control.type = "range"; for (const key of ["min","max","step"]) control[key] = specification[key];
      }
      control.value = specification.value; values[specification.key] = Number(control.value);
      const output = document.createElement("output"); outputs[specification.key] = { output, specification };
      label.append(control,output); controls.append(label);
      control.addEventListener("input", () => { values[specification.key] = Number(control.value); draw(0); });
    }
    const legend = document.createElement("div"); legend.className = "dsp-legend";
    for (const label of (kind==="measurements" ? [words.reconstructed,words.sample,words.truePeak] : kind==="continuity" ? [words.trackA,words.trackB,words.trim] : kind==="tone" ? [words.source,words.output,words.reserve] : ["channels","resampling","dither"].includes(kind) ? [words.source,words.output] : kind==="spectrum" ? [words.bands] : [words.source, words.output, words.guard])) { const span = document.createElement("span"); span.textContent = label; legend.append(span); }
    const readout = document.createElement("div"); readout.className = "dsp-readout"; readout.setAttribute("role", "status");
    const disclaimer = document.createElement("p"); disclaimer.className = "dsp-disclaimer"; disclaimer.textContent = words.disclaimer;
    container.append(title,intro,chart,legend,controls,readout,disclaimer);
    const context = canvas.getContext("2d");
    let width = 600, height = 245, visible = false, lastFrame = 0, animationId = 0;
    const resize = () => { const rect = canvas.getBoundingClientRect(); width = Math.max(180, rect.width); height = rect.height || 245; const ratio = Math.min(2,window.devicePixelRatio || 1); canvas.width = Math.round(width*ratio); canvas.height = Math.round(height*ratio); context.setTransform(ratio,0,0,ratio,0,0); draw(0); };
    const color = { input:"#c1a5ff", output:"#52def4", guard:"#ff9ba6", grid:"#463c56", text:"#c8bbda" };
    const labelText = (text,x,y,size=10,align="left") => { context.font=`${size}px ui-monospace,Consolas,monospace`; context.fillStyle=color.text; context.textAlign=align; context.fillText(text,x,y); };
    const curve = (fn,yScale,colorValue,phase=0) => { context.strokeStyle=colorValue; context.lineWidth=2; context.beginPath(); for(let i=0;i<=400;i++){const x=i/400;const y=fn(x,phase); const px=40+x*(width-58),py=height/2-y*yScale;if(i===0)context.moveTo(px,py);else context.lineTo(px,py);}context.stroke(); };
    const grid = (range=1) => { context.clearRect(0,0,width,height); context.strokeStyle=color.grid; context.lineWidth=1; for(const value of [-range,0,range]) {const y=height/2-value*(height-58)/2/range;context.beginPath();context.moveTo(40,y);context.lineTo(width-18,y);context.stroke();labelText(String(Number(value.toFixed(2))),31,y+3,9,"right");}labelText(words.time,width-18,height-10,9,"right");};
    const signal = sampleSignal;
    const outputText = text => { if(readout.textContent!==text)readout.textContent=text; };
    function draw(time) {
      for(const [key,{output,specification}] of Object.entries(outputs)) output.textContent=specification.options ? specification.options[values[key]] : `${Number(values[key].toFixed(2))} ${specification.unit}`;
      const phase = reduced.matches ? 0 : time/7000;
      grid();
      if(kind==="measurements") {
        const signalAt=x=>Math.sin(2*Math.PI*(values.cycles*x+values.phase))*.95;
        curve(signalAt,(height-58)/2,color.input);
        let samplePeak=0;context.strokeStyle=color.output;context.lineWidth=1;context.beginPath();
        for(let sample=0;sample<=24;sample++){const x=sample/24,value=signalAt(x),px=40+x*(width-58),py=height/2-value*(height-58)/2;samplePeak=Math.max(samplePeak,Math.abs(value));if(sample)context.lineTo(px,py);else context.moveTo(px,py);}
        context.stroke();
        for(let sample=0;sample<=24;sample++){const x=sample/24,value=signalAt(x);context.fillStyle=color.output;context.beginPath();context.arc(40+x*(width-58),height/2-value*(height-58)/2,2.7,0,Math.PI*2);context.fill();}
        context.strokeStyle=color.guard;context.setLineDash([4,4]);for(const sign of [-1,1]){context.beginPath();context.moveTo(40,height/2-sign*.95*(height-58)/2);context.lineTo(width-18,height/2-sign*.95*(height-58)/2);context.stroke();}context.setLineDash([]);
        if(!reduced.matches){const cursor=(time%4000)/4000;context.fillStyle=color.guard;context.beginPath();context.arc(40+cursor*(width-58),height/2-signalAt(cursor)*(height-58)/2,4,0,Math.PI*2);context.fill();}
        outputText(`${words.samplePeak} : ${db(samplePeak).toFixed(2)} dBFS · ${words.truePeak} : ${db(.95).toFixed(2)} dB. ${french?"Des maxima peuvent se trouver entre les points PCM. Le modèle illustre le principe ; les LUFS et le compteur de crête vraie d’Aède suivent les méthodes décrites ci-dessous.":"Maxima can lie between PCM points. This model illustrates the principle; Aède LUFS and true-peak meters use the methods described below."}`);
      } else if(kind==="normalization") {
        const gain=safeGain(values.level,values.target,values.peak),factor=amplitude(gain),peak=amplitude(values.peak);
        curve((x,p)=>signal(x,p)*peak,(height-58)/2,color.input,phase);curve((x,p)=>signal(x,p)*peak*factor,(height-58)/2,color.output,phase);
        outputText(`${words.gain} : ${gain.toFixed(1)} dB · ${words.output} : ${(values.level+gain).toFixed(1)} LUFS · ${gain < values.target-values.level ? words.limited : words.available}. ${french?"Aède utilise actuellement une cible fixe de −18 LUFS ; le réglage de cible ici sert uniquement à comprendre le calcul.":"Aède currently uses a fixed −18 LUFS target; this target control only explains the calculation."}`);
      } else if(kind==="tone") {
        context.clearRect(0,0,width,height); const left=42,right=width-20,top=26,bottom=height-38;const y=dbValue=>top+(12-dbValue)/36*(bottom-top);
        context.strokeStyle=color.grid; for(const value of [-24,-12,0,12]){context.beginPath();context.moveTo(left,y(value));context.lineTo(right,y(value));context.stroke();labelText(`${value}`,34,y(value)+3,9,"right");}
        for(const frequency of [20,120,1000,4000,20000]){const x=left+Math.log(frequency/20)/Math.log(1000)*(right-left);labelText(frequency>=1000?`${frequency/1000}k`:String(frequency),x,bottom+19,9,"center");}
        context.beginPath();context.strokeStyle=color.output;context.lineWidth=2.5;for(let i=0;i<=350;i++){const frequency=20*1000**(i/350),x=left+i/350*(right-left),response=toneResponse(frequency,values.bass,values.treble);if(i)context.lineTo(x,y(response));else context.moveTo(x,y(response));}context.stroke();
        context.strokeStyle=color.input;context.setLineDash([5,4]);context.beginPath();context.moveTo(left,y(0));context.lineTo(right,y(0));context.stroke();context.setLineDash([]);labelText("dB",left,16);labelText("Hz",right,height-9,9,"right");
        context.strokeStyle=color.guard;context.setLineDash([2,5]);context.beginPath();context.moveTo(left,y(-reserve(values.bass,values.treble)));context.lineTo(right,y(-reserve(values.bass,values.treble)));context.stroke();context.setLineDash([]);
        if(!reduced.matches){const cursor=(time%7000)/7000,frequency=20*1000**cursor;context.fillStyle=color.output;context.beginPath();context.arc(left+cursor*(right-left),y(toneResponse(frequency,values.bass,values.treble)),4,0,Math.PI*2);context.fill();}
        outputText(`${words.reserve} : −${reserve(values.bass,values.treble)} dB · ${words.bass} : 120 Hz · ${words.treble} : 4 kHz. ${french?"Courbe totale, préampli inclus. À zéro, les filtres sont contournés.":"Combined response includes the preamp. At zero, both filters are bypassed."}`);
      } else if(kind==="channels") {
        const channel=specifications.channels[0].options[values.channel],weights=downmix(channel);
        context.clearRect(0,0,width,height);const positions=[{x:width*.2,y:height/2,label:channel},{x:width*.76,y:height*.32,label:"L"},{x:width*.76,y:height*.74,label:"R"}];
        for(let side=0;side<2;side++){const start=positions[0],end=positions[side+1];context.strokeStyle=weights[side]?color.output:color.grid;context.lineWidth=weights[side]?2:1;context.setLineDash(weights[side]?[]:[4,5]);context.beginPath();context.moveTo(start.x,start.y);context.lineTo(end.x,end.y);context.stroke();context.setLineDash([]);labelText(weights[side].toFixed(3),width*.48,(start.y+end.y)/2-8,11,"center");if(weights[side]&&!reduced.matches){const point=(time%2100)/2100;context.fillStyle=color.output;context.beginPath();context.arc(start.x+(end.x-start.x)*point,start.y+(end.y-start.y)*point,4,0,Math.PI*2);context.fill();}}
        for(const point of positions){context.fillStyle="#29213a";context.strokeStyle=color.input;context.beginPath();context.arc(point.x,point.y,24,0,Math.PI*2);context.fill();context.stroke();labelText(point.label,point.x,point.y+4,11,"center");}
        outputText(`${channel} → L ${weights[0].toFixed(3)} · R ${weights[1].toFixed(3)}. ${channel==="LFE"?words.omitted:words.coefficients}.`);
      } else if(kind==="resampling") {
        const rate=Number(specifications.resampling[1].options[values.rate]),kept=values.frequency<rate*.48;
        const cycles=values.frequency/1600;curve((x,p)=>Math.sin(2*Math.PI*(cycles*x+p))*.78,(height-58)/2,color.input,phase);curve((x,p)=>kept?Math.sin(2*Math.PI*(cycles*x+p))*.78:0,(height-58)/2,color.output,phase);
        for(let sample=0;sample<=40;sample++){const x=sample/40,y=kept?Math.sin(2*Math.PI*(cycles*x+phase))*.78:0;context.fillStyle=color.output;context.beginPath();context.arc(40+x*(width-58),height/2-y*(height-58)/2,2.5,0,Math.PI*2);context.fill();}
        outputText(`${values.frequency} Hz · ${rate/1000} kHz → Nyquist ${rate/2000} kHz. ${kept?words.preserved:words.filtered}. ${french?"Transition de filtre schématisée ; les points illustrent les échantillons, sans reproduire le convertisseur Rust.":"Filter transition is schematic; dots illustrate samples without reproducing the Rust converter."}`);
      } else if(kind==="dither") {
        const step=2/2**values.bits;const noise=i=>(((Math.sin(i*12.9898)*43758.5453)%1+1)%1-((Math.sin(i*78.233)*12345.6789)%1+1)%1)*step;
        const original=x=>signal(x,phase)*.16;
        curve(original,(height-58)/.7,color.input);curve(x=>Math.round((original(x)+(values.dither?noise(Math.floor(x*140)+1):0))/step)*step,(height-58)/.7,color.output);
        outputText(`${values.bits} bits · ${words.error} ≈ ±${(step/2).toFixed(4)} ${french?"sans dither":"without dither"}. ${values.dither?words.more+".":""} ${words.noteBits}`);
      } else if(kind==="continuity") {
        context.clearRect(0,0,width,height);
        const left=28,right=width-20,span=right-left,middle=left+span/2,reset=Boolean(values.transition||values.format);
        const encoded=(x,label)=>{
          const blockWidth=span*.45,padding=blockWidth*.12;
          labelText(label,x+blockWidth/2,20,10,"center");
          context.fillStyle="#52445f";context.fillRect(x,34,blockWidth,26);
          context.fillStyle=color.input;context.fillRect(x+padding,34,blockWidth-2*padding,26);
          for(const start of [x,x+blockWidth-padding]){context.strokeStyle=color.guard;context.lineWidth=1.5;context.beginPath();context.moveTo(start+2,36);context.lineTo(start+padding-2,58);context.moveTo(start+padding-2,36);context.lineTo(start+2,58);context.stroke();}
        };
        encoded(left,words.trackA);encoded(left+span*.55,words.trackB);labelText(words.trim,middle,82,width<350?8:10,"center");
        const gap=reset?span*.045:0,startB=middle+gap,endA=middle-gap,yCenter=155,scale=29;
        context.strokeStyle=color.grid;context.beginPath();context.moveTo(left,yCenter);context.lineTo(right,yCenter);context.stroke();
        const drawSegment=(start,end,cycles,colorValue)=>{context.strokeStyle=colorValue;context.lineWidth=2;context.beginPath();for(let i=0;i<=180;i++){const x=i/180,px=start+x*(end-start),py=yCenter-Math.sin(x*2*Math.PI*cycles)*scale;if(!i)context.moveTo(px,py);else context.lineTo(px,py);}context.stroke();};
        drawSegment(left,endA,3,color.input);drawSegment(startB,right,4,color.output);
        context.strokeStyle=reset?color.guard:color.output;context.setLineDash(reset?[4,4]:[]);context.beginPath();context.moveTo(middle,106);context.lineTo(middle,199);context.stroke();context.setLineDash([]);
        if(!reduced.matches){const progress=(time%6000)/6000,x=left+progress*span;if(!reset||Math.abs(x-middle)>gap){context.fillStyle=progress<.5?color.input:color.output;context.beginPath();context.arc(x,155,4,0,Math.PI*2);context.fill();}}
        labelText(reset?words.reset:words.session,middle,height-12,width<350?9:11,"center");
        outputText(`${words.trim}. ${reset?words.reset:words.session}: ${values.transition?words.skip:values.format?words.different:words.natural+" · "+words.compatible}. ${words.hardware}.`);
      } else if(kind==="spectrum") {
        context.clearRect(0,0,width,height);context.strokeStyle=color.grid;context.beginPath();context.moveTo(35,height-35);context.lineTo(width-15,height-35);context.stroke();
        const barWidth=(width-55)/24;for(let band=0;band<24;band++){const frequency=20*1000**(band/23);let energy=0;for(let harmonic=1;harmonic<=3;harmonic++)energy+=Math.exp(-(((Math.log(frequency/(values.frequency*harmonic)))/.19)**2))/harmonic;const movement=reduced.matches?1:.9+.1*Math.sin(time/500+band);const bar=Math.min(1,energy)*movement*(height-65);context.fillStyle=band%2?color.output:color.input;context.fillRect(38+band*barWidth,height-35-bar,barWidth-3,bar);}labelText("20 Hz",35,height-10);labelText("20 kHz",width-15,height-10,10,"right");
        outputText(`${words.bands}. ${values.frequency} Hz + ${values.frequency*2} / ${values.frequency*3} Hz. ${french?"Distribution schématique d’un signal harmonique ; ceci n’est pas une mesure de votre musique.":"Schematic distribution of a harmonic signal; this is not a measurement of your music."}`);
      } else if(kind==="metering") {
        const scale=(height-58)/3.4;grid(1.7);curve((x,p)=>signal(x,p)*values.amplitude,scale,color.input,phase);curve((x,p)=>clamp(signal(x,p)*values.amplitude,-1,1),scale,color.output,phase);
        for(const limit of [-1,1]){context.strokeStyle=color.guard;context.setLineDash([5,4]);context.beginPath();context.moveTo(40,height/2-limit*scale);context.lineTo(width-18,height/2-limit*scale);context.stroke();context.setLineDash([]);}
        outputText(`${words.peak} : ${db(values.amplitude).toFixed(1)} dBFS ${french?"(amplitude du modèle)":"(model amplitude)"}. ${values.amplitude>1?words.clipping:words.clean}. ${words.noLimiter}.`);
      } else {
        context.clearRect(0,0,width,height);
        const stages=specifications.pipeline[0].options,columns=4,boxWidth=(width-48)/columns,boxHeight=27;
        for(let index=0;index<stages.length;index++){
          const x=24+(index%columns)*boxWidth,y=18+Math.floor(index/columns)*48;
          context.fillStyle=index===values.stage?"#514064":"#252030";context.strokeStyle=index===values.stage?color.output:color.grid;context.lineWidth=1;context.fillRect(x,y,boxWidth-7,boxHeight);context.strokeRect(x,y,boxWidth-7,boxHeight);
          labelText(stages[index],x+(boxWidth-7)/2,y+17,width<350?8:10,"center");
          if(!reduced.matches&&Math.floor((time%4800)/600)===index){context.fillStyle=color.output;context.beginPath();context.arc(x+boxWidth-15,y+5,2.5,0,Math.PI*2);context.fill();}
        }
        const factor=values.stage>=3?amplitude(values.gain):1,guarded=values.stage>=5,yCenter=height-67,scale=37;
        const drawSignal=(processed,colorValue)=>{context.strokeStyle=colorValue;context.lineWidth=2;context.beginPath();for(let i=0;i<=320;i++){const x=i/320;let sample=signal(x,phase)*.5;if(processed){sample*=factor;if(guarded)sample=clamp(sample,-1,1);}const px=24+x*(width-48),py=yCenter-sample*scale;if(!i)context.moveTo(px,py);else context.lineTo(px,py);}context.stroke();};
        drawSignal(false,color.input);drawSignal(true,color.output);
        if(guarded){context.strokeStyle=color.guard;context.setLineDash([4,4]);for(const limit of [-1,1]){context.beginPath();context.moveTo(24,yCenter-limit*scale);context.lineTo(width-24,yCenter-limit*scale);context.stroke();}context.setLineDash([]);}
        labelText(stages[values.stage],width-24,height-14,10,"right");
        outputText(`${stages.join(" → ")}. ${words.stage} : ${stages[values.stage]}. ${french?"La courbe illustre le gain à partir de son étape et l’écrêtage après le garde-fou. Les autres traitements ont leurs guides dédiés.":"The curve illustrates gain from its stage and clipping after the guard. Other processing has dedicated guides."}`);
      }
    }
    new ResizeObserver(resize).observe(container);
    const stopAnimation = () => { if(animationId)cancelAnimationFrame(animationId);animationId=0; };
    const startAnimation = () => { if(!animationId&&visible&&!document.hidden&&!reduced.matches)animationId=requestAnimationFrame(frame); };
    const observer = new IntersectionObserver(entries => { visible = entries[0].isIntersecting; if(visible)startAnimation();else stopAnimation(); },{threshold:.1});observer.observe(container);
    function frame(time) { animationId=0;if(!visible||document.hidden||reduced.matches)return;if(time-lastFrame>32){draw(time);lastFrame=time;}startAnimation(); }
    document.addEventListener("visibilitychange",()=>{if(document.hidden)stopAnimation();else startAnimation();});
    reduced.addEventListener("change",()=>{stopAnimation();draw(0);startAnimation();});
    resize();
  }
})();
