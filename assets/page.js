(() => {
  const plot = document.querySelector('.plot');
  const panel = document.getElementById('panel');
  const legend = document.getElementById('legend');
  const following = document.getElementById('following');
  const bar = document.getElementById('bar');
  const roleOf = el => (el.getAttribute('class') || '').split(' ')[1] || '';

  // The drawing can be replaced under the reader — another axis, another scale — so nothing below
  // holds on to an element of it for longer than one drawing lasts. What the reader chose is kept
  // as facts about the run (a process, an event, a kind), and applied again to each new drawing;
  // and the pointer is listened to on the frame around the drawing, which stays.
  let svg = null, marked = [], labels = [];

  // Detail blocks, as written. Pretty-printed when they happen to be JSON, verbatim when not:
  // the format never promised they were anything in particular.
  const details = new Map();
  for (const s of document.querySelectorAll('script.detail')) {
    let text = s.textContent;
    try { text = JSON.stringify(JSON.parse(text), null, 2); } catch (_) {}
    details.set(s.dataset.event, text);
  }

  const hide = () => panel.classList.remove('on');
  const show = (ev, role) => {
    const text = details.get(ev);
    if (!text) return hide();
    panel.textContent = '';
    const b = document.createElement('b');
    b.textContent = role || ('event ' + ev);
    panel.append(b, document.createTextNode(text));
    panel.classList.add('on');
  };
  plot.addEventListener('mouseover', e => {
    const el = e.target.closest('[data-event]');
    if (el) show(el.dataset.event, roleOf(el)); else hide();
  });
  plot.addEventListener('mouseleave', hide);

  // One toggle per kind, so a dense plot can have a whole kind taken out of it. The names are the
  // ones the style sheet selects by, so what you learn in one transfers to the other.
  const skip = ['lifeline', 'gridline', 'tick', 'participant', 'elapsed', ''];
  const dimmed = new Set();
  const dim = () => {
    for (const el of svg.querySelectorAll('.p')) el.classList.toggle('dim', dimmed.has(roleOf(el)));
  };
  const kinds = () => {
    const roles = [...new Set([...svg.querySelectorAll('.p')].map(roleOf))]
      .filter(r => !skip.includes(r)).sort();
    legend.textContent = '';
    for (const role of roles) {
      const b = document.createElement('button');
      b.textContent = role;
      b.setAttribute('aria-pressed', String(!dimmed.has(role)));
      b.addEventListener('click', () => {
        if (!dimmed.delete(role)) dimmed.add(role);
        b.setAttribute('aria-pressed', String(!dimmed.has(role)));
        dim();
      });
      legend.append(b);
    }
    if (dimmed.size) dim();
  };

  // Following one thing through the run. A click on a process keeps what involves it. A click on
  // an event keeps what it depends on and what depends on it — what happened before it and what
  // happened after it, in the sense of the partial order a run has — and fades what is concurrent
  // with it. The facts come from the derivation, not from the picture.
  const run = JSON.parse(document.getElementById('run').textContent);
  const idle = 'click a process to keep what involves it, or an event to keep its causes and consequences';
  let chosen = null;

  const involving = name => {
    const keep = new Set();
    for (const m of run.messages) if (m.from === name || m.to === name) keep.add(m.e);
    // What belongs to no process — the network, the run itself — is never faded: it bears on all.
    for (const p of run.points) if (p.lane === name || p.lane === null) keep.add(p.e);
    return keep;
  };

  const around = ev => {
    // `since`: per process, the instant from which it has been reached by the event.
    // `until`: per process, the instant up to which it contributed to the event.
    const since = new Map(), until = new Map();
    const lower = (l, t) => (!since.has(l) || t < since.get(l)) && (since.set(l, t), true);
    const raise = (l, t) => (!until.has(l) || t > until.get(l)) && (until.set(l, t), true);

    const msg = run.messages.find(m => m.e === ev);
    const pt = run.points.find(p => p.e === ev);
    if (msg) {
      // A message is caused by what its sender had seen when it left, and causes what its
      // receiver does from the moment it lands — if it lands.
      if (msg.left) raise(msg.from, msg.depart);
      if (msg.landed) lower(msg.to, msg.arrive);
    } else if (pt && pt.lane !== null) {
      raise(pt.lane, pt.t);
      lower(pt.lane, pt.t);
    } else {
      return null;
    }
    // A message that landed carries its sender's past forward and its receiver's future back.
    for (let moved = true; moved;) {
      moved = false;
      for (const m of run.messages) {
        if (m.landed && since.has(m.from) && m.depart >= since.get(m.from)) moved = lower(m.to, m.arrive) || moved;
        if (m.landed && until.has(m.to) && m.arrive <= until.get(m.to)) moved = raise(m.from, m.depart) || moved;
      }
    }
    const keep = new Set([ev]);
    for (const m of run.messages) {
      if (m.left && since.has(m.from) && m.depart >= since.get(m.from)) keep.add(m.e);
      if (m.landed && until.has(m.to) && m.arrive <= until.get(m.to)) keep.add(m.e);
    }
    for (const p of run.points) {
      // What belongs to no process is never faded, here either.
      if (p.lane === null) { keep.add(p.e); continue; }
      if (since.has(p.lane) && p.t >= since.get(p.lane)) keep.add(p.e);
      if (until.has(p.lane) && p.t <= until.get(p.lane)) keep.add(p.e);
    }
    return keep;
  };

  // Paint the drawing in hand according to what is chosen. Called on every choice, and again on
  // every new drawing.
  const follow = () => {
    let keep = null, text = idle;
    if (chosen && chosen.lane !== undefined) {
      keep = involving(chosen.lane);
      text = 'what involves ' + chosen.lane + ' — click it again to clear';
    } else if (chosen) {
      keep = around(chosen.ev);
      text = 'causes and consequences of the ' + (chosen.role || 'event') + ' you clicked — click it again to clear';
    }
    for (const l of labels) l.classList.toggle('on', !!chosen && chosen.lane === l.textContent.trim());
    for (const el of marked) el.classList.toggle('off', keep !== null && !keep.has(+el.dataset.event));
    following.textContent = text;
  };

  plot.addEventListener('click', e => {
    const label = e.target.closest('.p.participant');
    const el = e.target.closest('[data-event]');
    if (label) {
      const name = label.textContent.trim();
      chosen = chosen && chosen.lane === name ? null : { lane: name };
    } else if (el) {
      const ev = +el.dataset.event;
      if (chosen && chosen.ev === ev) chosen = null;
      else if (around(ev) === null) return;
      else chosen = { ev, role: roleOf(el) };
    } else {
      chosen = null;
    }
    follow();
  });
  document.addEventListener('keydown', e => { if (e.key === 'Escape') { chosen = null; follow(); } });

  // Stretching time, cheaply. Moving the slider asks for the run to be laid out again at the new
  // scale, which is the real answer and takes a moment; until it comes, this moves what is already
  // drawn, so the hand on the slider sees something follow it. Whatever has a duration is
  // stretched — an arrow, a lifeline, a span — and whatever marks an instant is carried along at
  // its own size. Labels keep the places they had, which is exactly what the second layout
  // corrects. Where the page cannot lay out again, this is all there is.
  const lasting = new Set(run.points.filter(p => p.lasts).map(p => p.e));
  const stretched = new Set(['arrow', 'lifeline', 'gridline', 'tick']);
  const num = (el, a) => +el.getAttribute(a);
  let origin = 0, boxW = 0, boxH = 0, shapes = [], factor = 1;

  // What each element is in the drawing in hand, read once: scaling always starts from there, so
  // going up and back down returns exactly where it began.
  const read = () => {
    const top = Math.min(...[...svg.querySelectorAll('.p.lifeline')].flatMap(l => [num(l, 'y1'), num(l, 'y2')]), Infinity);
    origin = Number.isFinite(top) ? top : 0;
    [, , boxW, boxH] = svg.getAttribute('viewBox').split(' ').map(Number);
    shapes = [];
    factor = 1;
    for (const el of svg.querySelectorAll('line, rect, circle, polygon, path, text')) {
      if (el.closest('defs')) continue;
      const kind = el.tagName.toLowerCase();
      const lasts = stretched.has(roleOf(el)) || lasting.has(+el.dataset.event);
      if (kind === 'line') shapes.push({ el, kind, lasts, y1: num(el, 'y1'), y2: num(el, 'y2') });
      else if (kind === 'rect') shapes.push({ el, kind, lasts: lasts || !el.classList.contains('p'), y: num(el, 'y'), h: num(el, 'height') });
      else if (kind === 'circle') shapes.push({ el, kind, y: num(el, 'cy') });
      else if (kind === 'polygon') shapes.push({ el, kind, pts: el.getAttribute('points').split(' ').map(p => p.split(',').map(Number)) });
      else if (kind === 'path') shapes.push({ el, kind, d: el.getAttribute('d').split(' ') });
      else {
        const turn = /rotate\(([-\d.]+) ([-\d.]+) ([-\d.]+)\)/.exec(el.getAttribute('transform') || '');
        shapes.push({ el, kind, y: num(el, 'y'), turn: turn && turn.slice(1).map(Number) });
      }
    }
  };

  const stretch = k => {
    const at = y => y <= origin ? y : origin + (y - origin) * k;
    const r4 = v => Math.round(v * 1e4) / 1e4;
    for (const s of shapes) {
      const el = s.el;
      if (s.kind === 'line') {
        if (s.lasts) { el.setAttribute('y1', r4(at(s.y1))); el.setAttribute('y2', r4(at(s.y2))); }
        else { const mid = (s.y1 + s.y2) / 2, d = at(mid) - mid; el.setAttribute('y1', r4(s.y1 + d)); el.setAttribute('y2', r4(s.y2 + d)); }
      } else if (s.kind === 'rect') {
        if (s.lasts) { el.setAttribute('y', r4(at(s.y))); el.setAttribute('height', r4(Math.max(at(s.y + s.h) - at(s.y), 0.5))); }
        else { const mid = s.y + s.h / 2; el.setAttribute('y', r4(s.y + at(mid) - mid)); }
      } else if (s.kind === 'circle') {
        el.setAttribute('cy', r4(at(s.y)));
      } else if (s.kind === 'polygon') {
        const mid = s.pts.reduce((a, p) => a + p[1], 0) / s.pts.length, d = at(mid) - mid;
        el.setAttribute('points', s.pts.map(p => p[0] + ',' + r4(p[1] + d)).join(' '));
      } else if (s.kind === 'path') {
        // Absolute `M x y L x y … Z`: every second number is a height.
        let odd = false;
        el.setAttribute('d', s.d.map(tok => {
          if (isNaN(tok)) { odd = false; return tok; }
          odd = !odd;
          return odd ? tok : r4(at(+tok));
        }).join(' '));
      } else {
        el.setAttribute('y', r4(at(s.y)));
        if (s.turn) {
          // A label rides its arrow's slope, and the slope changes with the scale.
          const [deg, cx, cy] = s.turn;
          const now = Math.atan(Math.tan(deg * Math.PI / 180) * k) * 180 / Math.PI;
          el.setAttribute('transform', 'rotate(' + r4(now) + ' ' + cx + ' ' + r4(at(cy)) + ')');
        }
      }
    }
    svg.setAttribute('viewBox', '0 0 ' + boxW + ' ' + r4(at(boxH)));
    factor = k;
  };

  // What the reader is looking at stays where it is: the instant at the middle of the window
  // before the change is put back at the middle after it.
  // The page turns off the browser's own attempt at the same thing (`overflow-anchor`): where a
  // browser has one, the two corrections add up and the view jumps by as much again.
  const rescale = k => {
    const box = svg.getBoundingClientRect();
    const px = box.width / boxW;
    const seen = ((window.innerHeight - bar.offsetHeight) / 2 - box.top) / px;
    const instant = seen <= origin ? seen : origin + (seen - origin) / factor;
    // Where to be afterwards is worked out before anything moves, and asked for outright. A
    // relative scroll goes wrong when the page shrinks: the browser first pulls the view back
    // inside the shorter page, and the step is then taken from the wrong place.
    const after = instant <= origin ? instant : origin + (instant - origin) * k;
    const target = window.scrollY + (after - seen) * px;
    stretch(k);
    window.scrollTo(0, target);
  };

  // Take the drawing now in the frame as the one in hand.
  const bind = () => {
    svg = plot.querySelector('svg');
    marked = [...svg.querySelectorAll('[data-event]')];
    labels = [...svg.querySelectorAll('.p.participant')];
    kinds();
    follow();
    read();
  };
  bind();

  // One band holds every control, fixed at the foot of the window. Its height depends on how many
  // kinds the run has, so it is measured, and the page leaves that much room under the drawing:
  // nothing of the run can end up behind it.
  const room = () => document.documentElement.style.setProperty('--bar', bar.offsetHeight + 'px');
  room();
  window.addEventListener('resize', room);

  // Laying the run out again, here. The page carries the document, its style sheet and the layout
  // itself — the same code that drew the first picture, compiled for the browser — so another axis
  // or another scale is a second run of the real thing: labels are placed again, not dragged.
  // It runs beside the page rather than in it, because a run of ten thousand messages takes
  // seconds, and a page that cannot scroll meanwhile reads as a page that has died. A request that
  // is overtaken is abandoned outright: its worker is stopped and another started.
  const source = JSON.parse(document.getElementById('source').textContent);
  const axes = [...document.querySelectorAll('#axis button')];
  const slider = document.querySelector('#scale input');
  const shown = document.querySelector('#scale output');
  let axis = source.axis;   // the axis asked for
  let want = 1;             // the scale asked for
  let laid = 1;             // the scale the drawing in hand was laid out at
  let bytes = null, worker = null, asked = null, serial = 0;

  const press = () => {
    for (const b of axes) b.setAttribute('aria-pressed', String(+b.dataset.axis === axis));
  };
  press();

  const beside = `
    const enc = new TextEncoder(), dec = new TextDecoder();
    let ready, doc, style;
    onmessage = e => {
      const m = e.data;
      if (m.bytes) {
        doc = m.doc; style = m.style;
        ready = WebAssembly.instantiate(m.bytes, {}).then(r => r.instance.exports);
        return;
      }
      ready.then(x => {
        const put = s => {
          const b = enc.encode(s), p = x.room(b.length);
          new Uint8Array(x.memory.buffer, p, b.length).set(b);
          return [p, b.length];
        };
        const [dp, dl] = put(doc), [sp, sl] = put(style);
        const head = x.draw(dp, dl, sp, sl, m.axis, m.scale);
        const view = new DataView(x.memory.buffer);
        const ptr = view.getUint32(head, true), len = view.getUint32(head + 4, true);
        const out = dec.decode(new Uint8Array(x.memory.buffer, ptr, len));
        x.release(dp, dl); x.release(sp, sl); x.release(ptr, len); x.release(head, 8);
        postMessage({ id: m.id, ok: out[0] === '+', text: out.slice(1) });
      }).catch(err => postMessage({ id: m.id, ok: false, text: String(err) }));
    };`;

  // Where the reader is, as an event rather than a height: under another axis the same height is
  // another moment. The event drawn nearest the middle of the window is put back where it was.
  const place = () => {
    const mid = (window.innerHeight - bar.offsetHeight) / 2;
    let best = null, gap = Infinity;
    for (const el of marked) {
      const r = el.getBoundingClientRect();
      const d = Math.abs((r.top + r.bottom) / 2 - mid);
      if (d < gap) { gap = d; best = el; }
    }
    if (!best) return null;
    const ev = best.dataset.event;
    const r = best.getBoundingClientRect();
    return { ev, nth: marked.filter(el => el.dataset.event === ev).indexOf(best), y: (r.top + r.bottom) / 2 };
  };
  const settle = was => {
    if (!was) return;
    const same = marked.filter(el => el.dataset.event === was.ev);
    const el = same[was.nth] || same[0];
    if (!el) return;
    const r = el.getBoundingClientRect();
    window.scrollTo(0, window.scrollY + (r.top + r.bottom) / 2 - was.y);
  };

  const answer = e => {
    const m = e.data;
    if (!asked || m.id !== asked.id) return;
    const done = asked;
    asked = null;
    delete bar.dataset.busy;
    if (!m.ok) { following.textContent = 'the run could not be laid out again: ' + m.text; return; }
    const was = place();
    plot.innerHTML = m.text;
    laid = done.scale;
    bind();
    // The slider may have moved on while this was being worked out; the next answer is on its way.
    if (want !== laid) stretch(want / laid);
    settle(was);
  };

  const ask = () => {
    if (!bytes) return;
    if (asked && worker) { worker.terminate(); worker = null; }
    if (!worker) {
      worker = new Worker(URL.createObjectURL(new Blob([beside], { type: 'text/javascript' })));
      worker.onmessage = answer;
      worker.postMessage({ bytes, doc: source.doc, style: source.style });
    }
    asked = { id: ++serial, axis, scale: want };
    bar.dataset.busy = '';
    worker.postMessage(asked);
  };

  // The layout travels in the page as text, compressed. Unpacked once, when the page opens, so the
  // first change of axis does not wait for it.
  (async () => {
    const packed = Uint8Array.from(atob(document.getElementById('layout').textContent.trim()), c => c.charCodeAt(0));
    const buf = await new Response(new Blob([packed]).stream().pipeThrough(new DecompressionStream('gzip'))).arrayBuffer();
    await WebAssembly.compile(buf);
    bytes = buf;
    for (const b of axes) b.disabled = false;
  })().catch(() => {
    // Nothing is lost that the page had before: the picture, the fading, the cheap stretch.
    for (const b of axes) b.title = 'this browser cannot lay the run out again, so the axis stays as the style sheet set it';
  });

  for (const b of axes) {
    b.addEventListener('click', () => {
      if (+b.dataset.axis === axis) return;
      axis = +b.dataset.axis;
      press();
      ask();
    });
  }

  let pending = false, later = null;
  slider.addEventListener('input', () => {
    // The slider runs on a logarithm, so halving and doubling are the same distance apart.
    want = Math.pow(2, +slider.value);
    shown.textContent = '×' + (want >= 10 ? want.toFixed(0) : want.toFixed(2));
    // A timer rather than an animation frame: a frame never comes while the page is in the
    // background, and the scale would silently stay where it was.
    if (!pending) {
      pending = true;
      setTimeout(() => { pending = false; rescale(want / laid); }, 16);
    }
    // And the real layout once the hand has stopped.
    clearTimeout(later);
    later = setTimeout(ask, 150);
  });
  slider.addEventListener('dblclick', () => { slider.value = 0; slider.dispatchEvent(new Event('input')); });
})();
