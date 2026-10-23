(() => {
  const stage = document.getElementById('ae-sillons');
  if (!stage) return;

  // Resolve from this script, so the same module works under /aede/ and /aede/en/.
  const threeUrl = new URL('vendor/three-0.180.0/three.module.min.js', document.currentScript.src);
  const fallback = stage.querySelector('.ae-sillons-fallback');
  const context = fallback.getContext('2d');
  const reduced = matchMedia('(prefers-reduced-motion: reduce)');
  const tau = Math.PI * 2;
  const state = {
    width: 0, height: 0, visible: false, frame: 0, last: 0,
    x: 0, y: 0, force: 0, targetX: 0, targetY: 0, targetForce: 0, pulse: 3,
  };
  let graphics = null, loading = false, contextLost = false;
  stage.dataset.renderer = 'fallback';

  function surface(u, radius) {
    const smooth = Math.min(1, radius);
    const z = .48 * Math.cos(radius * 1.7)
      + .12 * Math.sin(u * 3 + radius) * smooth * smooth * (3 - 2 * smooth);
    return [radius * Math.cos(u), radius * Math.sin(u), z];
  }

  function rotation() {
    return [.94 + state.y * .14, -.12 + state.x * .2, -.16];
  }

  function rotate(point) {
    let [x, y, z] = point;
    const [a, b, c] = rotation();
    [y, z] = [y * Math.cos(a) - z * Math.sin(a), y * Math.sin(a) + z * Math.cos(a)];
    [x, z] = [x * Math.cos(b) + z * Math.sin(b), -x * Math.sin(b) + z * Math.cos(b)];
    return [x * Math.cos(c) - y * Math.sin(c), x * Math.sin(c) + y * Math.cos(c), z];
  }

  function distance() {
    // Leave room for the deformed outer rings, including on narrow phones.
    return Math.max(8.8, 9.1 / (state.width / state.height));
  }

  function deform(point) {
    let [x, y, z] = point;
    const dx = x - state.x * 3.25, dy = y - state.y * 2.3;
    const d = Math.hypot(dx, dy), influence = Math.exp(-d * d * .87) * state.force * .9;
    z += influence * .95;
    x += dx * influence * .095;
    y += dy * influence * .075;
    if (state.pulse < 2.1) z += Math.sin(d * 7.5 - state.pulse * 8) * Math.exp(-state.pulse * 1.6) * .21;
    return [x, y, z];
  }

  function drawFallback() {
    if (!context) return;
    const { width, height } = state, ratio = Math.min(devicePixelRatio || 1, 1.5);
    if (fallback.width !== Math.round(width * ratio) || fallback.height !== Math.round(height * ratio)) {
      fallback.width = Math.round(width * ratio);
      fallback.height = Math.round(height * ratio);
    }
    context.setTransform(ratio, 0, 0, ratio, 0, 0);
    context.clearRect(0, 0, width, height);
    const focal = height / (2 * Math.tan(38 * Math.PI / 360)), faces = [];
    const project = p => [width / 2 + p[0] * focal / (distance() - p[2]), height / 2 - p[1] * focal / (distance() - p[2])];
    for (let band = 0; band < 48; band++) {
      const step = 2.53 / 48, radius = .32 + (band + .5) * step, half = step * .33;
      for (let segment = 0; segment < 72; segment++) {
        const u = segment / 72 * tau, next = (segment + 1) / 72 * tau;
        const points = [[u, radius - half], [next, radius - half], [next, radius + half], [u, radius + half]]
          .map(([angle, r]) => rotate(deform(surface(angle, r))));
        const a = points[0], b = points[1], c = points[3];
        const ab = b.map((value, i) => value - a[i]), ac = c.map((value, i) => value - a[i]);
        const n = [ab[1] * ac[2] - ab[2] * ac[1], ab[2] * ac[0] - ab[0] * ac[2], ab[0] * ac[1] - ab[1] * ac[0]];
        const light = Math.abs((n[0] * -.35 + n[1] * .52 + n[2] * .78) / (Math.hypot(...n) || 1));
        faces.push({ points: points.map(project), z: points.reduce((sum, p) => sum + p[2], 0) / 4, light });
      }
    }
    faces.sort((a, b) => a.z - b.z);
    for (const face of faces) {
      const value = Math.round(27 + Math.pow(face.light, .8) * 193);
      context.fillStyle = `rgb(${value + 5},${value + 5},${Math.max(0, value - 4)})`;
      context.beginPath();
      face.points.forEach((p, i) => i ? context.lineTo(...p) : context.moveTo(...p));
      context.closePath();
      context.fill();
    }
  }

  function render() {
    if (!state.width || !state.height || document.hidden) return;
    if (!graphics || contextLost) { drawFallback(); return; }
    const { renderer, scene, camera, material, mesh } = graphics;
    material.uniforms.uPointer.value.set(state.x * 3.25, state.y * 2.3);
    material.uniforms.uForce.value = state.force * .9;
    material.uniforms.uPulse.value = state.pulse;
    mesh.rotation.set(...rotation());
    renderer.render(scene, camera);
  }

  function resize() {
    const box = stage.getBoundingClientRect();
    state.width = box.width;
    state.height = box.height;
    if (graphics && box.width && box.height) {
      graphics.renderer.setSize(box.width, box.height, false);
      graphics.camera.aspect = box.width / box.height;
      graphics.camera.position.z = distance();
      graphics.camera.updateProjectionMatrix();
    }
    render();
  }

  function stop() {
    if (state.frame) cancelAnimationFrame(state.frame);
    state.frame = 0;
    state.last = 0;
  }

  function wake() {
    if (!state.visible || document.hidden || state.frame) return;
    if (reduced.matches) {
      state.x = state.targetX; state.y = state.targetY; state.force = state.targetForce; state.pulse = 3;
      render();
      return;
    }
    state.last = 0;
    state.frame = requestAnimationFrame(tick);
  }

  function tick(now) {
    state.frame = 0;
    if (!state.visible || document.hidden) return;
    const dt = state.last ? Math.min((now - state.last) / 1000, .04) : 1 / 60;
    state.last = now;
    const ease = 1 - Math.exp(-dt * 7.6);
    for (const key of ['x', 'y', 'force']) state[key] += (state['target' + key[0].toUpperCase() + key.slice(1)] - state[key]) * ease;
    state.pulse = Math.min(3, state.pulse + dt);
    render();
    const delta = Math.abs(state.x - state.targetX) + Math.abs(state.y - state.targetY) + Math.abs(state.force - state.targetForce);
    // No idle render loop: only an explicit gesture and its settling use frames.
    if (delta > .0008 || state.pulse < 2.15) state.frame = requestAnimationFrame(tick);
  }

  function reset() {
    state.targetX = 0; state.targetY = 0; state.targetForce = 0;
    wake();
  }

  stage.addEventListener('pointermove', event => {
    const box = stage.getBoundingClientRect();
    state.targetX = Math.max(-1, Math.min(1, (event.clientX - box.left) / box.width * 2 - 1));
    state.targetY = Math.max(-1, Math.min(1, 1 - (event.clientY - box.top) / box.height * 2));
    state.targetForce = 1;
    wake();
  });
  stage.addEventListener('pointerleave', reset);
  stage.addEventListener('pointercancel', reset);
  stage.addEventListener('blur', reset);
  stage.addEventListener('click', () => { state.pulse = 0; state.targetForce = 1; wake(); });
  stage.addEventListener('keydown', event => {
    const directions = { ArrowLeft: [-.23, 0], ArrowRight: [.23, 0], ArrowUp: [0, .23], ArrowDown: [0, -.23] };
    if (directions[event.key]) {
      event.preventDefault();
      const [x, y] = directions[event.key];
      state.targetX = Math.max(-1, Math.min(1, state.targetX + x));
      state.targetY = Math.max(-1, Math.min(1, state.targetY + y));
      state.targetForce = 1;
      wake();
    } else if (event.key === 'Escape') reset();
  });

  const vertex = `
    uniform vec2 uPointer;
    uniform float uForce;
    uniform float uPulse;
    varying vec3 vNormal;
    varying vec3 vView;
    varying vec3 vModel;
    varying vec2 vBand;
    void main() {
      vec3 p = position;
      vec2 delta = p.xy - uPointer;
      float d = length(delta);
      float influence = exp(-d*d*.87)*uForce;
      p.z += influence*.95;
      p.xy += delta*influence*vec2(.095,.075);
      float envelope = exp(-uPulse*1.6)*(1.-step(2.1,uPulse));
      p.z += sin(d*7.5-uPulse*8.)*envelope*.21;
      vec2 grad = -1.653*delta*influence;
      grad += delta/max(d,.001)*(7.5*cos(d*7.5-uPulse*8.))*envelope*.21;
      vec3 adjusted = normal;
      adjusted.xy -= grad*normal.z;
      vNormal = normalize(normalMatrix*adjusted);
      vec4 view = modelViewMatrix*vec4(p,1.);
      vView = view.xyz; vModel = p; vBand = uv;
      gl_Position = projectionMatrix*view;
    }
  `;
  const fragment = `
    uniform vec3 uAccent;
    varying vec3 vNormal;
    varying vec3 vView;
    varying vec3 vModel;
    varying vec2 vBand;
    void main() {
      vec3 n = normalize(vNormal);
      if (!gl_FrontFacing) n = -n;
      vec3 view = normalize(-vView);
      vec3 light = normalize(vec3(-.55,.82,1.4));
      vec3 fill = normalize(vec3(1.2,-.1,.65));
      float diffuse = max(dot(n,light),0.);
      float side = max(dot(n,fill),0.);
      float fresnel = pow(1.-max(dot(n,view),0.),2.3);
      float shine = pow(max(dot(n,normalize(light+view)),0.),28.);
      float soft = pow(max(dot(n,normalize(fill+view)),0.),16.);
      vec3 base = mix(vec3(.72,.745,.70),uAccent*.75,.055+.04*sin(vBand.y*6.283+vModel.z));
      vec3 col = base*(.16+diffuse*.74+side*.22);
      col += vec3(1.,.98,.90)*shine*.82 + uAccent*soft*.14;
      col += mix(vec3(.16),uAccent*.38,.55)*fresnel*.35;
      gl_FragColor = vec4(pow(max(col,vec3(0.)),vec3(.4545)),1.);
    }
  `;

  function geometry(THREE) {
    const bands = 48, segments = 220, positions = [], normals = [], uv = [], indices = [];
    for (let band = 0; band < bands; band++) {
      const step = 2.53 / bands, radius = .32 + (band + .5) * step, half = step * .33;
      const offset = positions.length / 3;
      for (let segment = 0; segment <= segments; segment++) {
        const u = segment / segments * tau;
        for (let side = 0; side < 2; side++) {
          const r = radius + (side ? half : -half), p = surface(u, r);
          const du = new THREE.Vector3(...surface(u + .001, r)).sub(new THREE.Vector3(...p));
          const dv = new THREE.Vector3(...surface(u, r + .001)).sub(new THREE.Vector3(...p));
          const normal = du.cross(dv).normalize();
          positions.push(...p); normals.push(normal.x, normal.y, normal.z); uv.push(side, band / bands);
        }
      }
      for (let segment = 0; segment < segments; segment++) {
        const a = offset + segment * 2;
        indices.push(a, a + 2, a + 1, a + 1, a + 2, a + 3);
      }
    }
    const result = new THREE.BufferGeometry();
    result.setAttribute('position', new THREE.Float32BufferAttribute(positions, 3));
    result.setAttribute('normal', new THREE.Float32BufferAttribute(normals, 3));
    result.setAttribute('uv', new THREE.Float32BufferAttribute(uv, 2));
    result.setIndex(indices);
    return result;
  }

  async function load() {
    if (loading) return;
    loading = true;
    let renderer, mesh, material;
    try {
      const THREE = await import(threeUrl.href);
      renderer = new THREE.WebGLRenderer({ antialias: true, alpha: true, powerPreference: 'low-power' });
      renderer.setPixelRatio(Math.min(devicePixelRatio || 1, 1.65));
      renderer.domElement.className = 'ae-sillons-gl';
      renderer.domElement.setAttribute('aria-hidden', 'true');
      const scene = new THREE.Scene(), camera = new THREE.PerspectiveCamera(38, 1, .1, 50);
      material = new THREE.ShaderMaterial({ vertexShader: vertex, fragmentShader: fragment, side: THREE.DoubleSide, uniforms: {
        uPointer: { value: new THREE.Vector2() }, uForce: { value: 0 }, uPulse: { value: 3 },
        uAccent: { value: new THREE.Color(getComputedStyle(stage).getPropertyValue('--ae-accent').trim()) },
      } });
      mesh = new THREE.Mesh(geometry(THREE), material);
      mesh.frustumCulled = false;
      scene.add(mesh);
      graphics = { renderer, scene, camera, mesh, material };
      stage.append(renderer.domElement);
      resize();
      stage.dataset.renderer = 'webgl';
      renderer.domElement.addEventListener('webglcontextlost', event => {
        event.preventDefault(); contextLost = true; stage.dataset.renderer = 'fallback'; render();
      });
      renderer.domElement.addEventListener('webglcontextrestored', () => {
        contextLost = false; resize(); stage.dataset.renderer = 'webgl'; wake();
      });
    } catch {
      // The illustration must never prevent the rest of the homepage from working.
      mesh?.geometry.dispose(); material?.dispose(); renderer?.dispose(); renderer?.domElement.remove();
      graphics = null;
      stage.dataset.renderer = 'fallback';
      render();
    }
  }

  new ResizeObserver(resize).observe(stage);
  new IntersectionObserver(entries => {
    state.visible = entries[0].isIntersecting;
    if (state.visible) { resize(); load(); wake(); } else stop();
  }, { threshold: 0 }).observe(stage);
  document.addEventListener('visibilitychange', () => {
    if (document.hidden) stop(); else { resize(); wake(); }
  });
  reduced.addEventListener('change', () => { stop(); wake(); });
  resize();
})();
