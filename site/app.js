(() => {
  const root=document.getElementById('aede-motion-concepts'),reduce=matchMedia('(prefers-reduced-motion: reduce)');
  const header=root.querySelector('.ae-header');
  // Match the sticky header after language, viewport or font changes.
  const alignAnchors=()=>document.documentElement.style.setProperty('--ae-header-height',`${header.getBoundingClientRect().height}px`);
  alignAnchors();new ResizeObserver(alignAnchors).observe(header);
  const tr=value=>window.aedeTranslate?.(value)??value;
  const state={mode:0,node:0},settings={motion:true};
  const scenes=[root.querySelector('.ae-sound'),root.querySelector('.ae-graph'),root.querySelector('.ae-preserve')];
  const descriptions=['Vos fichiers, vos paroles. Une écoute à votre mesure.','Artistes, crédits, œuvres. Chaque lien ouvre une nouvelle piste.','Copie vérifiée, intégrité, analyses. Vos originaux restent intacts.'];
  const canvas=root.querySelector('#ae-sound-canvas'),ctx=canvas.getContext('2d'),graph=root.querySelector('#ae-graph-canvas'),gx=graph.getContext('2d');
  let w=0,h=0,t=0,last=0,lastDraw=0,animationId=0,active=false,rx=0,ry=0,targetX=0,targetY=0;
  const nodes=[
    {title:'Échos',type:'Album',x:-.08,y:-.02,detail:'Échos : un album relié à son artiste, sa production et ses pistes.'},
    {title:'Trio Aster',type:'Artiste',x:-.75,y:-.72,detail:'Trio Aster est l’artiste de l’album Échos.'},
    {title:'A. Morel',type:'Production',x:-.77,y:.64,detail:'A. Morel est crédité à la production de l’album.'},
    {title:'Lisières',type:'Piste',x:.58,y:-.02,detail:'Lisières est une piste de l’album et une interprétation de l’œuvre Rivages.'},
    {title:'Nora Vale',type:'Guitare',x:.80,y:-.77,detail:'Nora Vale participe à Lisières à la guitare.'},
    {title:'Rivages',type:'Œuvre',x:.80,y:.68,detail:'L’œuvre Rivages est interprétée sur Lisières.'},
    {title:'E. Marin',type:'Composition',x:-.02,y:.90,detail:'E. Marin est crédité à la composition de Rivages.'}
  ];
  const edges=[{a:1,b:0,label:'artiste'},{a:2,b:0,label:'produit'},{a:0,b:3,label:'contient'},{a:4,b:3,label:'guitare'},{a:3,b:5,label:'interprète'},{a:6,b:5,label:'compose'}];
  const graphNodes=root.querySelector('.ae-graph-nodes');
  nodes.forEach((n,i)=>{const b=document.createElement('button');b.type='button';b.className='ae-node cursor-interaction';b.setAttribute('aria-label',n.title+', '+tr(n.type));b.setAttribute('aria-pressed',String(i===0));const name=document.createElement('strong');name.textContent=n.title;const role=document.createElement('small');role.textContent=n.type;b.append(name,role);b.addEventListener('click',()=>{state.node=i;graphNodes.querySelectorAll('button').forEach((x,k)=>x.setAttribute('aria-pressed',String(k===i)));root.querySelector('.ae-detail').textContent=n.detail;drawGraph();});graphNodes.append(b);});
  const bars=Array.from({length:30},(_,i)=>6+30*(.35+.65*Math.abs(Math.sin(i*1.83)))*Math.exp(-Math.pow((i-15)/12,2)));
  root.querySelectorAll('.ae-file-wave').forEach(el=>bars.forEach(height=>{const bar=document.createElement('span');bar.style.height=height+'px';el.append(bar);}));
  function size(c){let b=c.parentElement.getBoundingClientRect();if(!b.width)b=root.querySelector('.ae-stage').getBoundingClientRect();const ratio=Math.min(devicePixelRatio||1,2);c.width=Math.round(b.width*ratio);c.height=Math.round(b.height*ratio);c.getContext('2d').setTransform(ratio,0,0,ratio,0,0);return b;}
  function resize(){const b=size(canvas);w=b.width;h=b.height;size(graph);positionNodes();drawSound();drawGraph();drawEq();}
  function graphPos(n){const small=w<460;let x=n.x,y=n.y;if(w<420){const positions=[[0,-.02],[-.94,-.80],[-.94,.32],[.94,-.02],[.94,-.80],[.94,.72],[-.94,.89]],i=nodes.indexOf(n);x=positions[i][0];y=positions[i][1];}return {x:w/2+x*(w-(small?100:150))/2,y:h/2-16+y*(h-(small?95:110))/2};}
  function positionNodes(){graphNodes.querySelectorAll('button').forEach((b,i)=>{const p=graphPos(nodes[i]);b.style.left=p.x+'px';b.style.top=p.y+'px';});}
  function drawGraph(){
    if(!w)return;gx.clearRect(0,0,w,h);const accent=getComputedStyle(root).getPropertyValue('--ae-accent').trim();
    edges.forEach(e=>{const a=graphPos(nodes[e.a]),b=graphPos(nodes[e.b]),lit=e.a===state.node||e.b===state.node;gx.beginPath();gx.moveTo(a.x,a.y);gx.lineTo(b.x,b.y);gx.strokeStyle=lit?accent:'#4c5055';gx.globalAlpha=lit?.65:.7;gx.lineWidth=1;gx.stroke();if(w<420&&((e.a===0&&e.b===3)||(e.a===2&&e.b===0)))return;const x=(a.x+b.x)/2,y=(a.y+b.y)/2;gx.font='11px Helvetica, Arial, sans-serif';const tw=gx.measureText(tr(e.label)).width;gx.fillStyle='#111213';gx.globalAlpha=1;gx.fillRect(x-tw/2-5,y-7,tw+10,15);gx.fillStyle=lit?'#d4d6cb':'#a4a6a7';gx.textAlign='center';gx.textBaseline='middle';gx.fillText(tr(e.label),x,y);});
    gx.globalAlpha=1;
  }
  function drawSound(){
    if(!w)return;
    ctx.clearRect(0,0,w,h);
    const time=reduce.matches?0:t,span=Math.min(w-32,700),left=(w-span)/2+rx*8,mid=h*.43+ry*10;
    const phase=(time*2.1)%1,kick=Math.exp(-phase*9),height=Math.min(h*.30,105);
    const palette=ctx.createLinearGradient(left,0,left+span,0);
    palette.addColorStop(0,'#d8f36a');palette.addColorStop(.48,'#49d9ff');palette.addColorStop(1,'#b69aff');
    ctx.strokeStyle='#303236';ctx.lineWidth=1;ctx.globalAlpha=.55;
    for(const y of [mid-height,mid,mid+height]){ctx.beginPath();ctx.moveTo(left,y);ctx.lineTo(left+span,y);ctx.stroke();}
    // The beat drives motion only: this is an illustration, not a measured spectrum.
    for(let ring=0;ring<2;ring++){
      const progress=(phase+ring*.5)%1,radius=22+progress*height;
      ctx.beginPath();ctx.arc(w/2,mid,radius,0,Math.PI*2);ctx.strokeStyle='#49d9ff';ctx.globalAlpha=(1-progress)*.16;ctx.stroke();
    }
    const count=w<460?42:64,step=span/count;
    for(let i=0;i<count;i++){
      const u=i/(count-1),bass=Math.exp(-Math.pow((u-.20)/.19,2)),body=Math.exp(-Math.pow((u-.57)/.28,2));
      const texture=.35+.65*Math.pow(Math.sin(i*1.71-time*3.8),2);
      const energy=Math.min(1,.08+texture*(.28+.40*body)+kick*.48*bass+.14*Math.sin(time*5+i*.47)**2);
      const amplitude=energy*height,x=left+(i+.5)*step,barWidth=Math.max(2,step*.56);
      ctx.fillStyle=palette;ctx.globalAlpha=.55+.25*energy;ctx.fillRect(x-barWidth/2,mid-amplitude,barWidth,amplitude);
      ctx.globalAlpha=.20+.13*energy;ctx.fillRect(x-barWidth/2,mid+3,barWidth,amplitude*.72);
      ctx.fillStyle='#f1f0eb';ctx.globalAlpha=.65;ctx.fillRect(x-barWidth/2,mid-amplitude-5,barWidth,2);
    }
    for(let trail=2;trail>=0;trail--){
      ctx.beginPath();
      for(let i=0;i<=300;i++){
        const u=i/300,envelope=Math.sin(Math.PI*u)**.7;
        const wave=.56*Math.sin(u*67-time*14+trail*.22)+.28*Math.sin(u*151+time*8)+.16*Math.sin(u*239-time*11);
        const y=mid-wave*envelope*(18+kick*29),x=left+u*span;
        if(!i)ctx.moveTo(x,y);else ctx.lineTo(x,y);
      }
      ctx.strokeStyle=trail?palette:'#f1f0eb';ctx.globalAlpha=trail?.16:1;ctx.lineWidth=trail?5-trail:2;ctx.stroke();
    }
    ctx.globalAlpha=1;
  }
  function animatePreserve(){
    if(reduce.matches||!settings.motion)return;
    root.querySelector('.ae-transfer>span').animate([{transform:'translateX(0)',opacity:0},{opacity:1,offset:.12},{transform:'translateX('+Math.max(0,root.querySelector('.ae-transfer').getBoundingClientRect().width-18)+'px)',opacity:0}],{duration:1300,easing:'ease-in-out',iterations:2});
    root.querySelector('.ae-file-copy').animate([{transform:'translateY(7px)',borderColor:'#303236'},{transform:'translateY(0)',borderColor:'#d8f36a'},{borderColor:'#303236'}],{duration:1800,easing:'ease-out'});
  }
  function setMode(mode){
    state.mode=mode;root.querySelectorAll('[data-mode]').forEach(b=>b.setAttribute('aria-pressed',String(Number(b.dataset.mode)===mode)));
    scenes.forEach((scene,i)=>scene.hidden=i!==mode);root.querySelector('.ae-detail').textContent=descriptions[mode];
    if(!reduce.matches&&settings.motion)scenes[mode].animate([{opacity:.25,transform:'translateY(9px)'},{opacity:1,transform:'translateY(0)'}],{duration:500,easing:'cubic-bezier(.2,.8,.2,1)'});
    if(mode===0)drawSound();if(mode===1)drawGraph();if(mode===2)animatePreserve();syncSoundMotion();
  }
  root.querySelectorAll('[data-mode]').forEach(b=>b.addEventListener('click',()=>setMode(Number(b.dataset.mode))));
  const serverVisual=root.querySelector('.ae-server-visual');let serverVisible=false,serverAnimations=[];
  function animateServer(){
    serverAnimations.forEach(a=>a.cancel());serverAnimations=[];
    if(reduce.matches||!settings.motion)return;
    const style=getComputedStyle(serverVisual),accent=style.getPropertyValue('--ae-accent').trim(),muted=style.getPropertyValue('--ae-muted').trim(),background=style.getPropertyValue('--ae-bg').trim(),signal=style.getPropertyValue('--ae-server-signal').trim();
    const timing={duration:2500,iterations:Infinity,easing:'linear'};
    const packet=(el,distance,delay)=>serverAnimations.push(el.animate([{opacity:0,transform:'translateY(0)'},{opacity:1,offset:.06},{opacity:1,transform:'translateY('+distance+'px)',offset:.30},{opacity:0,transform:'translateY('+distance+'px)',offset:.36},{opacity:0,transform:'translateY('+distance+'px)'}],{...timing,delay}));
    packet(root.querySelector('.ae-packet-source'),30,0);
    serverAnimations.push(root.querySelector('.ae-server-center').animate([{borderColor:'#303236',outlineColor:'transparent'},{borderColor:signal,outlineColor:signal,offset:.08},{borderColor:signal,outlineColor:signal,offset:.30},{borderColor:'#303236',outlineColor:'transparent',offset:.42},{borderColor:'#303236',outlineColor:'transparent'}],{...timing,delay:550}));
    packet(root.querySelector('.ae-packet-client'),24,850);
    serverVisual.querySelectorAll('.ae-server-steps>span').forEach((el,i)=>serverAnimations.push(el.animate([{color:muted,backgroundColor:'transparent'},{color:background,backgroundColor:accent,offset:.08},{color:background,backgroundColor:accent,offset:.30},{color:muted,backgroundColor:'transparent',offset:.42},{color:muted,backgroundColor:'transparent'}],{...timing,delay:i*550})));
  }
  function syncServerMotion(){
    if(reduce.matches||!settings.motion){serverAnimations.forEach(a=>a.cancel());serverAnimations=[];return;}
    if(serverVisible&&!document.hidden){if(!serverAnimations.length)animateServer();serverAnimations.forEach(a=>a.play());}else serverAnimations.forEach(a=>a.pause());
  }
  const serverObserver=new IntersectionObserver(entries=>{serverVisible=entries[0].isIntersecting;syncServerMotion();},{threshold:.08});serverObserver.observe(serverVisual);
  document.addEventListener('visibilitychange',syncServerMotion);
  canvas.addEventListener('pointermove',e=>{if(reduce.matches||!settings.motion)return;const r=canvas.getBoundingClientRect();targetX=(e.clientX-r.left)/r.width-.5;targetY=(e.clientY-r.top)/r.height-.5;});
  canvas.addEventListener('pointerleave',()=>{targetX=targetY=0;});
  new ResizeObserver(resize).observe(root.querySelector('.ae-stage'));
  function soundCanAnimate(){return active&&!document.hidden&&state.mode===0&&!reduce.matches&&settings.motion;}
  function syncSoundMotion(){
    if(soundCanAnimate()){if(!animationId){last=0;animationId=requestAnimationFrame(frame);}}
    else if(animationId){cancelAnimationFrame(animationId);animationId=0;last=0;}
  }
  function frame(now){
    animationId=0;if(!soundCanAnimate())return;
    t+=last?Math.min((now-last)/1000,.05):0;last=now;
    if(now-lastDraw>=1000/30){rx+=(targetX-rx)*.12;ry+=(targetY-ry)*.12;drawSound();lastDraw=now;}
    animationId=requestAnimationFrame(frame);
  }
  document.addEventListener('visibilitychange',syncSoundMotion);
  new IntersectionObserver(entries=>{active=entries[0].isIntersecting;syncSoundMotion();},{threshold:.08}).observe(root.querySelector('.ae-stage'));
  const cli={
    scan:{command:'$ aede scan ~/Music',comment:'# Tout commence avec votre dossier Musique.',output:'Lecture des fichiers…\nConstruction du catalogue…\n\n 3 albums\n 9 pistes\n 4 artistes\n\nAudio et tags inchangés.',caption:'Vos dossiers deviennent un catalogue de liens musicaux.',question:'« Comment commencer avec mon dossier Musique ? »'},
    query:{command:'$ aede query "loved played:0"',comment:'# Vos favoris encore inécoutés.',output:'01  Lisières       Trio Aster\n02  Rivages        Nora Vale\n03  La traversée   Trio Aster\n\n3 pistes dans cette sélection.',caption:'Les favoris que vous n’avez pas encore écoutés.',question:'« Quels favoris n’ai-je pas encore écoutés ? »'},
    fetch:{command:'$ aede fetch --credits --lyrics --covers',comment:'# Le contexte vient compléter la collection.',output:'+ Crédits     MusicBrainz\n+ Paroles     LRCLIB\n+ Pochette    Cover Art Archive\n\nSources conservées.\nAudio et tags inchangés.',caption:'Enrichissement volontaire, sans remplacer les tags locaux.',question:'« Qui joue sur cet album, et où sont ses paroles ? »'},
    play:{command:'$ aede play ~/Music --normalize track',comment:'# Votre collection, prête à être écoutée.',output:'Lecture locale\n\n  Normalisation : piste\n  Graves / aigus : à plat\n  Spectre       : actif\n\nLes écoutes rejoignent votre historique.',caption:'Lecture d’un fichier, dossier, playlist ou sélection locale.',question:'« Comment écouter ma collection depuis le terminal ? »'},
    copy:{command:'$ aede copy /Volumes/Baladeur --query "loved" --verify',comment:'# Une sélection pour le baladeur.',output:'Sélection : favoris\n\n✓ Lisières.flac  copie relue\n✓ Rivages.flac   CRC32 identique\n\nOriginaux préservés.',caption:'Conversion des sources sans perte possible avec --compress.',question:'« Comment préparer ma musique pour la route ? »'},
    check:{command:'$ aede check',comment:'# Vérifier les sommes intégrées aux fichiers.',output:'✓ Lisières.flac  CRC valides\n✓ Rivages.ogg    CRC valides\n\nFLAC : trames contrôlées\nOgg  : pages contrôlées\n\nAutres formats : contrôle CRC indisponible.',caption:'Contrôle d’intégrité FLAC / Ogg, complémentaire de l’analyse audio.',question:'« Mes fichiers ont-ils subi des dégâts ? »'}
  };
  root.querySelectorAll('[data-cli]').forEach(b=>b.addEventListener('click',()=>{const item=cli[b.dataset.cli];root.querySelectorAll('[data-cli]').forEach(x=>x.setAttribute('aria-pressed',String(x===b)));root.querySelector('#ae-command').textContent=item.command;root.querySelector('#ae-command-result').textContent=item.output;root.querySelector('#ae-terminal-comment').textContent=item.comment;root.querySelector('#ae-command-caption').textContent=item.caption;root.querySelector('.ae-question').textContent=item.question;}));
  const eq=root.querySelector('#ae-eq-canvas'),ex=eq.getContext('2d'),bass=root.querySelector('#ae-bass'),treble=root.querySelector('#ae-treble');let actualBass=0,actualTreble=0,eqStart=0,fromBass=0,fromTreble=0;
  function drawEq(){
    const rect=eq.getBoundingClientRect();if(!rect.width)return;const ratio=Math.min(devicePixelRatio||1,2);eq.width=rect.width*ratio;eq.height=rect.height*ratio;ex.setTransform(ratio,0,0,ratio,0,0);const W=rect.width,H=rect.height,pad=23;
    ex.strokeStyle='#303236';ex.lineWidth=1;ex.beginPath();ex.moveTo(pad,H*.52);ex.lineTo(W-pad,H*.52);ex.stroke();ex.fillStyle='#a4a6a7';ex.font='12px Helvetica, Arial, sans-serif';ex.textAlign='left';ex.fillText('20 Hz',pad,H-13);ex.textAlign='right';ex.fillText('20 kHz',W-pad,H-13);ex.textAlign='left';ex.fillText('+6',pad,22);ex.fillText('−6',pad,H-34);
    ex.beginPath();for(let i=0;i<=170;i++){const u=i/170,value=actualBass/(1+Math.exp((u-.23)*17))+actualTreble/(1+Math.exp((.78-u)*17)),x=pad+u*(W-pad*2),y=H*.52-value*6;if(i===0)ex.moveTo(x,y);else ex.lineTo(x,y);}ex.strokeStyle=getComputedStyle(eq).getPropertyValue('--ae-accent').trim();ex.lineWidth=1.6;ex.stroke();
  }
  function toneChange(){root.querySelector('#ae-bass-value').textContent=(Number(bass.value)>0?'+':'')+bass.value+' dB';root.querySelector('#ae-treble-value').textContent=(Number(treble.value)>0?'+':'')+treble.value+' dB';fromBass=actualBass;fromTreble=actualTreble;eqStart=performance.now();const targetBass=Number(bass.value),targetTreble=Number(treble.value);function tick(now){const u=reduce.matches?1:Math.min(1,(now-eqStart)/350);actualBass=fromBass+(targetBass-fromBass)*u;actualTreble=fromTreble+(targetTreble-fromTreble)*u;drawEq();if(u<1)requestAnimationFrame(tick);}requestAnimationFrame(tick);}
  bass.addEventListener('input',toneChange);treble.addEventListener('input',toneChange);new ResizeObserver(drawEq).observe(eq);
  let favorite=false,appTab='tracks';const appDetail=root.querySelector('#ph-detail');
  const appViews={
    tracks:'<div class="ph-track"><span>01</span>Lisières<small>4:12</small></div><div class="ph-track"><span>02</span>Rivages<small>5:38</small></div><div class="ph-track"><span>03</span>La traversée<small>3:56</small></div>',
    credits:'<div class="ph-credit">Trio Aster<small>Artiste</small></div><div class="ph-credit">Nora Vale<small>Guitare · Lisières</small></div><div class="ph-credit">A. Morel<small>Production</small></div><p class="ph-source">Exemple fictif de crédits attribués.</p>',
    lyrics:'<p class="ph-lyrics">Les jours passent, les échos restent.<br>Un horizon au creux des mains.<small>Paroles fictives pour cette maquette.</small></p>',
    analysis:'<div class="ph-measures"><div>Sonie intégrée<strong>−14.2 LUFS</strong></div><div>Crête vraie<strong>−1.1 dBTP</strong></div><div>Profondeur mesurée<strong>24 bits</strong></div><div>FLAC audio MD5<strong>Concordant</strong></div></div><p class="ph-source">Analysé par <a class="ae-inline-link" href="https://craft-and-code.github.io/FlacCompagnon/" target="_blank" rel="noopener noreferrer">FlacCompagnon</a> · Valeurs illustratives.</p>'
  };
  function setAppTab(tab){appTab=tab;root.querySelectorAll('[data-app-tab]').forEach(b=>b.setAttribute('aria-pressed',String(b.dataset.appTab===tab)));appDetail.innerHTML=appViews[tab];}
  function setAppNav(nav){root.querySelectorAll('[data-app-nav]').forEach(b=>b.setAttribute('aria-pressed',String(b.dataset.appNav===nav)));root.querySelector('#ph-album').hidden=nav!=='collection';root.querySelector('#ph-artists').hidden=nav!=='artists';root.querySelector('#ph-favorites').hidden=nav!=='favorites';root.querySelector('#ph-favorite-status').textContent=favorite?'Échos · Trio Aster — ajouté à vos favoris dans ce concept.':'Aucun favori. Vous pouvez marquer Échos depuis la collection.';}
  root.querySelectorAll('[data-app-tab]').forEach(b=>b.addEventListener('click',()=>setAppTab(b.dataset.appTab)));
  root.querySelectorAll('[data-app-nav]').forEach(b=>b.addEventListener('click',()=>setAppNav(b.dataset.appNav)));
  root.querySelector('#ph-favorite').addEventListener('click',()=>{favorite=!favorite;root.querySelector('#ph-favorite').setAttribute('aria-pressed',String(favorite));root.querySelector('#ph-favorite>span').textContent=favorite?'Favori ajouté':'Favori';});
  root.querySelector('.ph-back').addEventListener('click',()=>setAppNav('collection'));setAppTab('tracks');
  // Keep full-width colour surfaces stationary and opaque so scrolling cannot expose dark seams.
  const revealObserver=new IntersectionObserver(entries=>entries.forEach(entry=>{
    if(!entry.isIntersecting)return;
    if(!reduce.matches&&settings.motion)for(const content of entry.target.children){
      content.animate([{transform:'translateY(12px)',opacity:.6},{transform:'translateY(0)',opacity:1}],{duration:650,easing:'cubic-bezier(.2,.8,.2,1)'});
    }
    revealObserver.unobserve(entry.target);
  }),{threshold:0,rootMargin:'0px 0px -64px 0px'});
  root.querySelectorAll('.ae-reveal').forEach(el=>revealObserver.observe(el));
  reduce.addEventListener('change',()=>{root.getAnimations({subtree:true}).forEach(a=>a.cancel());serverAnimations=[];syncServerMotion();syncSoundMotion();drawSound();drawGraph();drawEq();});
  window.aedeLocalize?.(root);
  new MutationObserver(records=>{for(const record of records){if(record.type==='characterData')window.aedeLocalize?.(record.target.parentElement);else for(const node of record.addedNodes)if(node.nodeType===1)window.aedeLocalize?.(node);else if(node.nodeType===3)window.aedeLocalize?.(node.parentElement);}}).observe(root,{childList:true,characterData:true,subtree:true});
  resize();
})();
