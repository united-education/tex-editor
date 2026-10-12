'use strict';
// Páginas: compila em segundo plano (tectonic + SyncTeX), mostra o PDF real
// ao lado e marca no texto onde começa cada página.

const PG = {
  layout: null, seq: 0, pdf: null, pdfSeq: 0, main: null, status: null,
  timer: null, poll: null, show: true, scale: 1, rendered: new Map(), observer: null,
  firstPage: new Map(), hoverIds: null, focusIds: null, lastFocusUnit: null,
};

// ------------------------------------------------------------ build
function scheduleBuild(delay = 1300) {
  clearTimeout(PG.timer);
  PG.timer = setTimeout(startBuild, delay);
}

async function startBuild() {
  if (!S.path) return;
  try {
    const st = await api('build', { path: S.path });
    setBuildStatus(st);
    if (!st.nomain) pollBuild();
  } catch (e) {
    setBuildStatus({ failed: true, errors: [{ msg: e.message }] });
  }
}

function pollBuild() {
  clearTimeout(PG.poll);
  PG.poll = setTimeout(async () => {
    if (!S.path) return;
    try {
      const st = await api('buildstatus', { path: S.path });
      if (st.main == null) return;
      setBuildStatus(st);
      if (st.ok_seq && st.ok_seq !== PG.seq) await loadLayout();
      if (st.running) pollBuild();
    } catch (_) { pollBuild(); }
  }, 900);
}

function setBuildStatus(st) {
  PG.status = st;
  const b = $('#build');
  b.className = 'build';
  if (st.running) { b.textContent = 'Compilando…'; b.classList.add('run'); }
  else if (st.nomain) { b.textContent = 'Sem arquivo principal'; }
  else if (st.failed) { b.textContent = '⚠ Erro na compilação'; b.classList.add('err'); }
  else if (st.ok_seq) { b.textContent = `${st.pages} págs · ${(st.ms / 1000).toFixed(0)} s`; b.classList.add('ok'); }
  else b.textContent = '—';
  b.title = st.main ? 'Compila ' + st.main : '';
  markErrors();
  refreshPlay();
}

async function loadLayout() {
  const L = await api('layout', { path: S.path });
  if (!L.seq) return;
  PG.layout = L;
  PG.seq = L.seq;
  PG.main = L.main;
  PG.firstPage = new Map();
  PG.rects = new Map();
  PG.lines = new Map();
  for (const b of L.blocks) {
    PG.rects.set(b.id, b.r);
    PG.lines.set(b.id, new Map((b.n || []).map(x => [x[0], x[1]])));
    PG.firstPage.set(b.id, Math.min(...b.r.map(r => r[0])));
  }
  applyGeometry();
  document.querySelectorAll("#page .capname").forEach(c => { c.textContent = `Figura ${figNumber(c.dataset.key)}: `; });
  loadPageKeys();
  placeBands();
  if (PG.show) await loadPdf();
}

// ------------------------------------------------------------- PDF
let pdfReady = null;
function pdfjs() {
  if (pdfReady) return pdfReady;
  pdfReady = (async () => {
    for (let i = 0; i < 100 && !window.pdfjsLib; i++) await new Promise(r => setTimeout(r, 50));
    if (!window.pdfjsLib) throw new Error('pdf.js não carregou (sem internet?)');
    // o worker vem da CDN; via blob ele roda na mesma origem
    const src = await (await fetch('https://cdnjs.cloudflare.com/ajax/libs/pdf.js/3.11.174/pdf.worker.min.js')).text();
    pdfjsLib.GlobalWorkerOptions.workerSrc = URL.createObjectURL(new Blob([src], { type: 'text/javascript' }));
    return pdfjsLib;
  })();
  return pdfReady;
}

async function loadPdf() {
  if (!PG.layout || PG.pdfSeq === PG.seq) { paintOverlays(); return; }
  const lib = await pdfjs().catch(e => { toast(e.message, true); return null; });
  if (!lib) return;
  const seq = PG.seq;
  const url = `/api/pdf?main=${encodeURIComponent(PG.main)}&seq=${seq}&t=${TOKEN}`;
  const doc = await lib.getDocument({ url, disableAutoFetch: false }).promise;
  if (seq !== PG.seq) return;
  const old = PG.pdf;
  PG.pdf = doc;
  PG.pdfSeq = seq;
  buildPageShells();
  if (old) old.destroy();
}

function paneWidth() { return Math.max(200, $('#pages-list').clientWidth - 28); }

function buildPageShells() {
  const list = $('#pages-list');
  const keepTop = list.scrollTop;
  const n = PG.pdf.numPages;
  const w = paneWidth();
  const pw = PG.layout.page_w, ph = PG.layout.page_h;
  PG.scale = w / pw;
  const oldShells = new Map([...list.querySelectorAll('.pg')].map(e => [+e.dataset.p, e]));
  list.textContent = '';
  if (PG.observer) PG.observer.disconnect();
  PG.observer = new IntersectionObserver(ents => {
    for (const en of ents) if (en.isIntersecting) renderPage(+en.target.dataset.p);
  }, { root: list, rootMargin: '600px 0px' });
  PG.rendered = new Map();
  for (let p = 1; p <= n; p++) {
    const sh = el('div', 'pg');
    sh.dataset.p = p;
    sh.style.width = w + 'px';
    sh.style.height = (ph * PG.scale) + 'px';
    // mantém o desenho antigo até o novo ficar pronto (sem piscar)
    const prev = oldShells.get(p) && oldShells.get(p).querySelector('canvas');
    if (prev) { prev.classList.add('stale'); sh.append(prev); }
    const lab = el('div', 'pg-n', String(p));
    const f = fillOf(p);
    if (f != null) lab.append(el('span', 'pg-fill', ` · ${Math.round(f * 100)}%`));
    sh.append(lab);
    list.append(sh);
    PG.observer.observe(sh);
  }
  list.scrollTop = keepTop;
  $('#pages-title').textContent = `Páginas (${n})`;
  paintOverlays();
}

async function renderPage(p) {
  if (PG.rendered.get(p) === PG.pdfSeq) return;
  PG.rendered.set(p, PG.pdfSeq);
  const seq = PG.pdfSeq;
  const page = await PG.pdf.getPage(p);
  const vp = page.getViewport({ scale: PG.scale * (window.devicePixelRatio || 1) });
  const c = document.createElement('canvas');
  c.width = vp.width; c.height = vp.height;
  c.style.width = '100%';
  await page.render({ canvasContext: c.getContext('2d'), viewport: vp }).promise;
  if (seq !== PG.pdfSeq) return;
  const sh = $(`#pages-list .pg[data-p="${p}"]`);
  if (!sh) return;
  sh.querySelectorAll('canvas').forEach(x => x.remove());
  sh.prepend(c);
}

// ocupação: do topo da mancha até o fim do conteúdo da página
function fillOf(p) {
  const L = PG.layout;
  if (!L || !L.fill) return null;
  const all = L.fill.filter(Boolean);
  if (!all.length || !L.fill[p - 1]) return null;
  const top = Math.min(...all.map(f => f[0]));
  const bottom = Math.max(...all.map(f => f[1]));
  return Math.max(0, Math.min(1, (L.fill[p - 1][1] - top) / (bottom - top)));
}

// ------------------------------------------------- blocos <-> páginas
function idsOfUnit(u) {
  if (!u) return [];
  if (u.dataset.begin != null && u.dataset.end != null) {
    const out = [];
    for (let i = +u.dataset.begin; i <= +u.dataset.end; i++) out.push(i);
    return out;
  }
  if (u.classList.contains('row')) return [...u.children].flatMap(idsOfUnit);
  if (u.dataset.id != null && !isNaN(+u.dataset.id)) return [+u.dataset.id];
  return [];
}

function rectsOf(ids) {
  const out = [];
  if (!PG.rects) return out;
  for (const id of ids) for (const r of PG.rects.get(id) || []) out.push(r);
  // um retângulo por página
  const per = new Map();
  for (const r of out) {
    const k = r[0];
    const x = per.get(k);
    if (!x) per.set(k, r.slice());
    else { x[1] = Math.min(x[1], r[1]); x[2] = Math.min(x[2], r[2]); x[3] = Math.max(x[3], r[3]); x[4] = Math.max(x[4], r[4]); }
  }
  return [...per.values()].sort((a, b) => a[0] - b[0]);
}

function drawRects(cls, rects) {
  document.querySelectorAll('#pages-list .ov .' + cls).forEach(e => e.remove());
  for (const r of rects) {
    const sh = $(`#pages-list .pg[data-p="${r[0]}"] .ov`);
    if (!sh) continue;
    const d = el('div', cls);
    const s = PG.scale;
    d.style.left = (r[1] * s - 3) + 'px';
    d.style.top = (r[2] * s - 3) + 'px';
    d.style.width = ((r[3] - r[1]) * s + 6) + 'px';
    d.style.height = ((r[4] - r[2]) * s + 6) + 'px';
    sh.append(d);
  }
}

// o PDF ao lado é só para ver: sem destaques, sem cliques
function paintOverlays() {
  return;
  drawRects('r-focus', PG.focusIds ? rectsOf(PG.focusIds) : []);
}
function paintHover(ids) {
  return;
  drawRects('r-hover', ids ? rectsOf(ids) : []);
}

function pageScrollTo(p, yBp) {
  const list = $('#pages-list');
  const sh = $(`#pages-list .pg[data-p="${p}"]`);
  if (!sh) return;
  const top = sh.offsetTop + (yBp || 0) * PG.scale - 60;
  if (top < list.scrollTop || top > list.scrollTop + list.clientHeight - 120) list.scrollTo({ top, behavior: 'smooth' });
}

// bloco do editor ganhou foco -> destaca no PDF
function onEditorFocusUnit(u) {
  return;
  if (!u || u === PG.lastFocusUnit) return;
  PG.lastFocusUnit = u;
  PG.focusIds = idsOfUnit(u);
  paintOverlays();
  const rs = rectsOf(PG.focusIds);
  if (rs.length && PG.show) pageScrollTo(rs[0][0], rs[0][2]);
}

function blockAt(p, x, y) {
  let best = null, area = Infinity;
  if (!PG.layout) return null;
  for (const b of PG.layout.blocks) {
    for (const r of b.r) {
      if (r[0] !== p || x < r[1] - 2 || x > r[3] + 2 || y < r[2] - 2 || y > r[4] + 2) continue;
      const a = (r[3] - r[1]) * (r[4] - r[2]);
      if (a < area) { area = a; best = b.id; }
    }
  }
  return best;
}

function editorElOf(id) {
  return $(`#page [data-id="${id}"]`) || $(`#page [data-begin="${id}"]`);
}

function onPageClick(e) {
  const sh = e.currentTarget;
  const p = +sh.dataset.p;
  const box = sh.getBoundingClientRect();
  const id = blockAt(p, (e.clientX - box.left) / PG.scale, (e.clientY - box.top) / PG.scale);
  if (id == null) return;
  const t = editorElOf(id);
  if (!t) return;
  t.scrollIntoView({ block: 'center', behavior: 'smooth' });
  t.classList.add('flash');
  setTimeout(() => t.classList.remove('flash'), 1200);
  if (t.matches('.blk[contenteditable="true"]')) setTimeout(() => setFlat(t, 0), 350);
  PG.focusIds = idsOfUnit(t);
  PG.lastFocusUnit = t;
  paintOverlays();
}

function onPageHover(e) {
  const sh = e.currentTarget;
  const box = sh.getBoundingClientRect();
  const id = blockAt(+sh.dataset.p, (e.clientX - box.left) / PG.scale, (e.clientY - box.top) / PG.scale);
  paintHover(id == null ? null : [id]);
}

// ------------------------------------------- modo página no editor
// O editor usa as medidas que o LaTeX informou (mancha, corpo, entrelinha,
// margens) e a mesma fonte do PDF. O fim de cada página aparece com o espaço
// que sobrou em branco, na mesma escala.
const BP = 72 / 72.27; // pt -> bp
PG.zoom = 0; // 0 = ajusta à largura
PG.px = 1; // px por bp

function applyGeometry() {
  const L = PG.layout;
  const g = L && L.geom;
  if (!g) return false;
  const page = $('#page');
  const avail = $('#doc').clientWidth - 36;
  const s = PG.zoom ? PG.zoom * 96 / 72 : Math.max(0.7, Math.min(2.4, avail / L.page_w));
  PG.px = s;
  const left = g.odd * BP, tw = g.tw * BP, top = g.top * BP;
  const right = L.page_w - left - tw;
  const bottom = L.page_h - top - g.th * BP;
  document.body.classList.add('pageview');
  Object.assign(page.style, {
    width: L.page_w * s + 'px',
    maxWidth: 'none',
    padding: `${top * s}px ${right * s}px ${bottom * s}px ${left * s}px`,
    fontSize: (g.fs * BP * s).toFixed(3) + 'px',
    lineHeight: String(g.bs / g.fs),
  });
  page.style.setProperty('--bs', (g.bs * BP * s).toFixed(3) + 'px');
  page.style.setProperty('--pt', (BP * s).toFixed(4) + 'px'); // 1 pt do TeX na tela
  page.style.setProperty('--ex', (0.4306 * g.fs * BP * s).toFixed(3) + 'px'); // 1 ex da Latin Modern
  page.style.setProperty('--pi', (g.pi * BP * s).toFixed(3) + 'px');
  page.style.setProperty('--ml', (left * s) + 'px');
  page.style.setProperty('--mr', (right * s) + 'px');
  const z = $('#zoom-v');
  if (z) z.textContent = Math.round(s * 72 / 96 * 100) + '%';
  return true;
}

function setZoom(z) {
  PG.zoom = z;
  if (applyGeometry()) placeBands();
}

const cm = bp => (bp / 72 * 2.54).toFixed(1).replace('.', ',') + ' cm';

function leafPages(e) {
  const rs = PG.rects && PG.rects.get(+e.dataset.id);
  if (!rs || !rs.length) return null;
  let a = Infinity, b = 0;
  for (const r of rs) { a = Math.min(a, r[0]); b = Math.max(b, r[0]); }
  // só parágrafos e itens se dividem entre páginas; num título ou no começo de
  // uma caixa, os registros de páginas seguintes são do que vem depois (a
  // moldura da caixa é desenhada no fim dela)
  if (!e.matches('.blk.para, .blk.item')) b = a;
  return { a, b, rs };
}

function isFloatLeaf(e) {
  const m = e.querySelector && e.querySelector(':scope > .macro[data-img]');
  if (m) { const M = (S.macros || {})[JSON.parse(m.dataset.meta).name]; return !!(M && M.float); }
  const b = e.classList.contains('raw') && S.blocks[+e.dataset.id];
  return !!(b && /^(figure|table)\*?$/.test(b.env || ''));
}

const NOT_UNIT = ['pband', 'glue', 'comment', 'pagemark', 'empty-doc', 'pv-top'];
function firstUnit(p) {
  for (const c of p.children) if (!NOT_UNIT.some(k => c.classList.contains(k))) return c;
  return null;
}
// onde a faixa entra: antes da caixa/lista/linha que começa a página
function bandAnchor(e) {
  let a = e.classList.contains('env-h') ? e.parentElement : e;
  for (let guard = 0; guard < 20; guard++) {
    const row = a.parentElement && a.parentElement.closest('#page .row');
    if (row) { a = row; continue; }
    const p = a.parentElement;
    if (p && (p.classList.contains('env-b') || p.classList.contains('list')) && firstUnit(p) === a) {
      a = p.classList.contains('list') ? p : p.parentElement;
      continue;
    }
    break;
  }
  return a;
}

function freeAt(k) {
  const L = PG.layout, g = L.geom;
  const f = L.fill[k - 1];
  return f ? Math.max(0, (g.top + g.th) * BP - f[1]) : 0;
}

// fim da página k (+ começo da próxima)
function makeBand(k, next, tag = 'div') {
  const L = PG.layout, g = L.geom, s = PG.px;
  const band = el(tag, (tag === 'span' ? 'pgx ' : '') + 'pband' + (L.stale ? ' stale' : ''));
  band.contentEditable = 'false';
  band.dataset.p = k;
  const free = freeAt(k);
  const bl = el(tag === 'span' ? 'span' : 'div', 'pb-blank');
  bl.style.height = (free * s).toFixed(1) + 'px';
  if (free > 8) {
    bl.classList.add('free');
    bl.append(el('span', 'pb-free', `${cm(free)} livres no fim da página ${k}`));
    bl.title = 'Espaço em branco no PDF. Arraste um bloco para cá para aproveitá-lo.';
  }
  const foot = el('span', 'pb-foot');
  foot.style.height = ((L.page_h - (g.top + g.th) * BP) * s).toFixed(1) + 'px';
  foot.append(el('span', 'pb-num', String(k)));
  band.append(bl, foot);
  if (next) {
    band.append(el('span', 'pb-gap'));
    const head = el('span', 'pb-head');
    head.style.height = (g.top * BP * s).toFixed(1) + 'px';
    const lab = el('span', 'pb-label', `Página ${next}`);
    if (next > k + 1) {
      const skipped = [];
      for (let p = k + 1; p < next; p++) skipped.push(p);
      const figs = skipped.every(p => PG.floatPages && PG.floatPages.has(p));
      const which = skipped.length > 1 ? `páginas ${k + 1}–${next - 1}` : `página ${k + 1}`;
      lab.append(el('span', 'pb-skip', ` (antes: ${which}, ${figs ? 'só com figuras' : 'com conteúdo de outro arquivo ou figuras'})`));
    }
    head.append(lab);
    band.dataset.next = next;
    band.append(head);
  }
  band.onclick = () => { if (PG.show) pageScrollTo(next || k, 0); };
  return band;
}

// alinha a faixa às bordas da página, mesmo dentro de caixas
function stretchBand(band) {
  const pr = $('#page').getBoundingClientRect();
  const br = band.getBoundingClientRect();
  band.style.marginLeft = (pr.left - br.left) + 'px';
  band.style.width = pr.width + 'px';
}

// linhas visuais de um parágrafo no editor: [{top, x}]
function editorLines(blk) {
  const r = document.createRange();
  r.selectNodeContents(blk);
  const tops = [];
  const lh = parseFloat(getComputedStyle(blk).lineHeight) || 16;
  for (const q of r.getClientRects()) {
    if (q.width < 1 || q.height > lh * 2.5) continue;
    if (!tops.some(t => Math.abs(t - q.top) < lh * 0.5)) tops.push(q.top);
  }
  return tops.sort((a, b) => a - b);
}

// primeiro caractere (nó de texto, offset) cuja linha começa em y >= top;
// busca binária nos caracteres (funciona fora da tela, ao contrário de
// caretRangeFromPoint)
function charAtLine(blk, top) {
  const w = document.createTreeWalker(blk, NodeFilter.SHOW_TEXT);
  const nodes = [];
  let total = 0, n;
  while ((n = w.nextNode())) { if (n.length && !n.parentElement.closest('.pgx, .fslot')) { nodes.push([n, total]); total += n.length; } }
  const at = i => {
    let lo = 0, hi = nodes.length - 1;
    while (lo < hi) { const m = (lo + hi + 1) >> 1; if (nodes[m][1] <= i) lo = m; else hi = m - 1; }
    return [nodes[lo][0], i - nodes[lo][1]];
  };
  const r = document.createRange();
  const topOf = i => {
    const [t, o] = at(i);
    r.setStart(t, o); r.setEnd(t, Math.min(o + 1, t.length));
    const q = r.getClientRects()[0];
    return q ? q.top : -Infinity;
  };
  if (!total || topOf(total - 1) < top) return null;
  let lo = 0, hi = total - 1;
  while (lo < hi) { const m = (lo + hi) >> 1; if (topOf(m) >= top) hi = m; else lo = m + 1; }
  // não corta no meio de um espaço: começa na palavra
  return at(lo);
}

// quebra de página no meio de um parágrafo: a faixa entra antes da linha certa
function splitLeaf(e, lp, from, to) {
  const g = PG.layout.geom;
  const bs = g.bs * BP, fs = g.fs * BP;
  const tops = editorLines(e);
  const cuts = [];
  let lines = 0;
  for (let k = from; k < to; k++) {
    const r = lp.rs.filter(x => x[0] === k);
    if (!r.length) continue;
    // linhas impressas nesta página: pelas linhas de base do SyncTeX; senão, pela altura
    const counted = PG.lines && PG.lines.get(+e.dataset.id) && PG.lines.get(+e.dataset.id).get(k);
    const y0 = Math.min(...r.map(x => x[2])), y1 = Math.max(...r.map(x => x[4]));
    lines += counted || Math.max(1, Math.round((y1 - y0 - fs) / bs) + 1);
    if (lines >= tops.length) break;
    const pos = charAtLine(e, tops[lines] - 1);
    if (pos) cuts.push({ k, pos });
  }
  // de trás para frente, para as posições não mudarem
  for (const c of cuts.reverse()) {
    const band = makeBand(c.k, c.k + 1, 'span');
    const [t, o] = c.pos;
    const atom = t.parentElement.closest('.atom, .macro');
    if (atom && e.contains(atom)) atom.before(band);
    else if (o > 0) t.splitText(o).before(band);
    else t.before(band);
  }
  return cuts.length > 0;
}

// --- quebras pelo texto do PDF: a primeira linha de cada página (pdf.js) é
// procurada no texto do editor; a faixa entra exatamente ali
const normTxt = s => s.normalize('NFKC').toLowerCase().replace(/[^\p{L}\p{N}]/gu, '');

async function loadPageKeys() {
  const seq = PG.seq, L = PG.layout;
  // a compilação é numerada por arquivo principal: a chave leva os dois
  const tag = PG.main + "#" + seq;
  if (!L || !L.geom || PG.keysSeq === tag || PG.keysLoading === tag) return;
  PG.keysLoading = tag;
  try {
    const lib = await pdfjs();
    const own = !(PG.pdf && PG.pdfSeq === seq);
    const doc = own ? await lib.getDocument({ url: `/api/pdf?main=${encodeURIComponent(PG.main)}&seq=${seq}&t=${TOKEN}` }).promise : PG.pdf;
    const g = L.geom, top = g.top * BP - 4, bot = (g.top + g.th) * BP + 4;
    const keys = new Map();
    for (let p = 1; p <= doc.numPages; p++) {
      const tc = await (await doc.getPage(p)).getTextContent();
      const rows = [];
      for (const it of tc.items) {
        if (!it.str || !it.str.trim()) continue;
        const y = L.page_h - it.transform[5], x = it.transform[4];
        if (y < top || y > bot) continue;
        const r = rows.find(q => Math.abs(q.y - y) < 3);
        if (r) r.items.push({ x, s: it.str }); else rows.push({ y, items: [{ x, s: it.str }] });
      }
      rows.sort((a, b) => a.y - b.y);
      keys.set(p, rows.slice(0, 6).map(r => normTxt(r.items.sort((a, b) => a.x - b.x).map(i => i.s).join(''))).filter(k => k.length >= 6));
      if (seq !== PG.seq || tag !== PG.main + "#" + PG.seq) return;
    }
    if (own) doc.destroy();
    PG.keys = keys;
    PG.keysSeq = tag;
    placeBands();
  } catch (_) { /* fica com o SyncTeX */ } finally { if (PG.keysLoading === tag) PG.keysLoading = 0; }
}

// texto do editor, só letras e números, com a origem de cada caractere
function textStream(leaves) {
  const map = [];
  let s = '';
  for (const e of leaves) {
    if (isFloatLeaf(e)) continue;
    if (!e.matches('.blk')) {
      // título de caixa, tabela: o texto conta, mas a quebra é antes do bloco
      const t = normTxt(e.classList.contains('env-h') ? (e.firstChild ? e.firstChild.textContent : '') : e.textContent);
      for (const c of t) { s += c; map.push([e, null, 0]); }
      continue;
    }
    // texto editável + o que as fórmulas e grandezas mostram (o PDF também tem)
    const w = document.createTreeWalker(e, NodeFilter.SHOW_TEXT | NodeFilter.SHOW_ELEMENT, {
      acceptNode: x => x.nodeType === 1
        ? (x.matches('.pgx, .mhide, .mname, .mraw, .mchip, .fslot') ? NodeFilter.FILTER_REJECT : x.matches('.atom.k-math, .atom.k-ce, .atom.k-si, .atom.k-sym') ? NodeFilter.FILTER_ACCEPT : NodeFilter.FILTER_SKIP)
        : (acceptText(x, e) ? NodeFilter.FILTER_ACCEPT : NodeFilter.FILTER_REJECT),
    });
    let n;
    while ((n = w.nextNode())) {
      if (n.nodeType === 1) {
        const t = normTxt(((n.querySelector('.katex-html') || n).textContent) || '');
        for (const c of t) { s += c; map.push([e, n, 0]); }
        continue;
      }
      const d = n.data;
      for (let i = 0; i < d.length; i++) {
        const t = normTxt(d[i]);
        for (const c of t) { s += c; map.push([e, n, i]); }
      }
    }
  }
  return { s, map };
}

// devolve a última página posta no editor, ou null se não deu
function textBreaks(leaves, firstPage, bands) {
  const { s, map } = textStream(leaves);
  if (!s) return null;
  const L = PG.layout;
  const cuts = [];
  let cursor = 0, last = firstPage;
  for (let p = firstPage + 1; p <= L.pages; p++) {
    // do mais exato ao mais solto: duas linhas inteiras, uma linha, o começo dela.
    // A linha pode começar com um rótulo que não está no texto ("(c)", "1.").
    const rows = PG.keys.get(p) || [];
    // uma página tem ~3 mil letras: um trecho curto só vale se estiver perto
    const find = k => {
      const i = k.length >= 8 ? s.indexOf(k, cursor) : -1;
      const win = k.length >= 40 ? 20000 : k.length >= 20 ? 9000 : 3500;
      return i >= 0 && i - cursor < win ? i : -1;
    };
    let best = -1, row = 0;
    const tries = [
      r => rows[r + 1] ? rows[r] + rows[r + 1] : null,
      r => rows[r].length >= 24 ? rows[r] : null,
      r => rows[r].slice(0, 24),
    ];
    for (const mk of tries) {
      for (let r = 0; r < rows.length && best < 0; r++) {
        const k0 = mk(r);
        if (!k0) continue;
        for (const cut of [0, 1, 2, 3]) {
          const i = find(k0.slice(cut));
          if (i >= 0) { best = i; row = r; break; }
        }
      }
      if (best >= 0) break;
    }
    if (best < 0) continue;
    // achou pela 2a, 3a linha: volta o tamanho das linhas de texto anteriores
    // (legendas de figura flutuante não estão no texto do editor)
    let back = 0;
    for (let q = 0; q < row; q++) if (!/^(figura|tabela)\d/.test(rows[q])) back += rows[q].length;
    best = Math.max(cursor, best - back);
    const [leaf, node, off] = map[best];
    const clean = best === 0 || map[best - 1][0] !== leaf;
    cuts.push({ k: last, next: p, leaf, node, off, clean });
    last = p;
    cursor = best + 1;
  }
  // de trás para frente: inserir não muda as posições anteriores
  for (const c of cuts.reverse()) {
    if (c.clean || !c.node) {
      const b = makeBand(c.k, c.next);
      bandAnchor(c.leaf).before(b);
      bands.push(b);
    } else {
      const b = makeBand(c.k, c.next, 'span');
      const atom = c.node.nodeType === 1 ? c.node : c.node.parentElement.closest('.atom, .macro');
      if (atom && c.leaf.contains(atom)) atom.before(b);
      else if (c.off > 0) c.node.splitText(c.off).before(b);
      else c.node.before(b);
      bands.push(b);
    }
  }
  return last;
}

// --- figuras flutuantes onde o LaTeX as pôs: o elemento vai (só na tela) para
// a página e a altura do PDF; um marcador guarda o lugar dele no texto
function restoreFloats() {
  for (const m of PG.moved || []) {
    // o elemento pode ter sido redesenhado dentro da ilha: usa o atual
    const cur = (m.slot && m.slot.firstElementChild) || m.el;
    if (m.home.isConnected) m.home.replaceWith(cur);
  }
  PG.moved = [];
  document.querySelectorAll('#page .fslot').forEach(s => { const p = s.parentNode; s.remove(); if (p && p.normalize) p.normalize(); });
}

function placeFloats(leaves) {
  const page = $("#page");
  const added = [];
  const pos = e => {
    const rs = PG.rects && PG.rects.get(+e.dataset.id);
    if (!rs || !rs.length) return null;
    const p = Math.min(...rs.map(r => r[0]));
    return { p, y: Math.min(...rs.filter(r => r[0] === p).map(r => r[2])) };
  };
  const flow = leaves.filter(e => !isFloatLeaf(e) && e.isConnected);
  for (const f of leaves.filter(e => isFloatLeaf(e) && e.isConnected)) {
    const fp = pos(f);
    if (!fp) continue;
    const unit = f.closest('#page > *') === f ? f : (unitOf(f) || f);
    // primeiro bloco do texto, nessa página, que fica abaixo da figura
    let target = null, inside = null;
    for (const e of flow) {
      const rs = (PG.rects.get(+e.dataset.id) || []).filter(r => r[0] === fp.p);
      if (!rs.length || Math.min(...rs.map(r => r[2])) <= fp.y) continue;
      const first = Math.min(...PG.rects.get(+e.dataset.id).map(r => r[0]));
      if (first < fp.p) {
        // parágrafo que vem da página anterior: a figura entra logo depois da quebra
        inside = e.querySelector(`.pgx.pband[data-next="${fp.p}"]`);
        if (!inside) continue;
      } else target = bandAnchor(e);
      break;
    }
    if (!target && !inside) {
      // abaixo de todo o texto da página: no fim dela
      let end = page.querySelector(`.pband[data-p="${fp.p}"]`);
      if (!end) {
        // página só de figuras (o LaTeX juntou as flutuantes numa página): a
        // faixa "k -> n" que pula essa página vira "k -> p" + figura + "p -> n"
        const skip = [...page.querySelectorAll('.pband[data-next]')].find(b => +b.dataset.p < fp.p && +b.dataset.next > fp.p);
        if (!skip || unit.contains(skip)) continue;
        const tag = skip.classList.contains('pgx') ? 'span' : 'div';
        const b1 = makeBand(+skip.dataset.p, fp.p, tag), b2 = makeBand(fp.p, +skip.dataset.next, tag);
        skip.replaceWith(b1);
        b1.after(b2);
        added.push(b1, b2);
        end = b2;
      }
      if (unit.contains(end)) continue;
      const s = slotFor(unit, end.classList.contains('pgx'));
      end.before(s.slot);
      PG.moved.push(s.rec);
      continue;
    }
    if (target === unit || (target && unit.contains(target))) continue;
    const s = slotFor(unit, !!inside);
    if (inside) inside.after(s.slot); else target.before(s.slot);
    PG.moved.push(s.rec);
  }
  return added;
}

// dentro de um parágrafo a figura vai numa "ilha" que o parágrafo não lê
function slotFor(unit, island = true) {
  const home = el('span', 'fhome');
  home.hidden = true;
  unit.replaceWith(home);
  const slot = el(island ? 'span' : 'div', 'fslot noread');
  slot.contentEditable = 'false';
  slot.append(unit);
  return { slot, rec: { el: unit, home, slot } };
}

function placeBands() {
  const page = $('#page');
  restoreFloats();
  page.querySelectorAll('.pband, .pgx, .pv-top').forEach(x => {
    const p = x.parentNode; x.remove(); if (p && p.normalize) p.normalize();
  });
  document.querySelectorAll('#page .floatp').forEach(x => { x.classList.remove('floatp'); x.removeAttribute('data-fp'); });
  const L = PG.layout;
  if (!L || !L.geom) return placeMarkers();
  if (!document.body.classList.contains('pageview') && !applyGeometry()) return;
  // guarda o cursor (as faixas mexem nos nós de texto)
  const act = curBlock();
  const off = act ? getFlat(act) : null;
  const leaves = [...page.querySelectorAll('[data-id]')].filter(e => !e.classList.contains('glue') && !e.classList.contains('comment') && !e.closest('.pgx'));
  let cur = null;
  const bands = [];
  PG.floatPages = new Set();
  for (const e of leaves) {
    const lp = isFloatLeaf(e) && leafPages(e);
    if (lp) for (const r of lp.rs) PG.floatPages.add(r[0]);
  }
  for (const e of leaves) {
    const lp = isFloatLeaf(e) && leafPages(e);
    // figura flutuante: o LaTeX decide a página
    if (lp) { e.classList.add('floatp'); e.dataset.fp = lp.a; }
  }
  const first = leaves.map(e => !isFloatLeaf(e) && leafPages(e)).find(Boolean);
  if (first) {
    const t = el('div', 'pv-top', `Página ${first.a}`);
    t.contentEditable = 'false';
    page.prepend(t);
  }
  // pelo texto do PDF (exato) ou, enquanto ele não chega, pelo SyncTeX
  const byText = PG.keys && PG.keysSeq === PG.main + "#" + PG.seq && first ? textBreaks(leaves, first.a, bands) : null;
  if (byText != null) cur = byText;
  else for (const e of leaves) {
    const lp = leafPages(e);
    if (!lp || isFloatLeaf(e)) continue;
    if (cur == null) cur = lp.a;
    if (lp.a > cur) {
      const b = makeBand(cur, lp.a);
      bandAnchor(e).before(b);
      bands.push(b);
      cur = lp.a;
    }
    if (lp.b > cur) {
      if (e.matches('.blk') && splitLeaf(e, lp, cur, lp.b)) bands.push(...e.querySelectorAll(':scope .pgx'));
      cur = lp.b;
    }
  }
  if (cur != null) {
    const end = makeBand(cur, null);
    end.classList.add('last');
    if (cur < L.pages) end.append(el('span', 'pb-rest', `Depois deste arquivo: páginas ${cur + 1}–${L.pages} (outros arquivos ou figuras).`));
    page.append(end);
    bands.push(end);
  }
  bands.push(...placeFloats(leaves));
  for (const b of bands) stretchBand(b);
  if (act && off != null && document.activeElement && act.contains(document.activeElement)) setFlat(act, off);
}

// ----------------------------------------------- marcas no editor
// (sem as medidas da página: só a linha "Página N")
function placeMarkers() {
  document.querySelectorAll('#page .pagemark').forEach(e => e.remove());
  const L = PG.layout;
  if (!L) return;
  let last = 0;
  const els = [...document.querySelectorAll('#page [data-id], #page .env[data-begin], #page .list[data-begin]')];
  for (const e of els) {
    const id = e.dataset.id != null ? +e.dataset.id : +e.dataset.begin;
    if (isNaN(id)) continue;
    const p = PG.firstPage.get(id);
    if (p == null || p <= last) continue;
    let anchor = e.classList.contains('env-h') ? e.parentElement : e;
    const row = anchor.closest('.row');
    if (row) anchor = row;
    if (anchor.previousElementSibling && anchor.previousElementSibling.classList.contains('pagemark')) { last = p; continue; }
    const m = el('div', 'pagemark' + (L.stale ? ' stale' : ''));
    m.contentEditable = 'false';
    m.dataset.p = p;
    m.append(el('span', 'pm-n', `Página ${p}`));
    if (p > 1) {
      const f = fillOf(p - 1);
      if (f != null) {
        const bar = el('span', 'pm-bar');
        const fillEl = el('i');
        fillEl.style.width = Math.round(f * 100) + '%';
        bar.append(fillEl);
        m.append(el('span', 'pm-prev', `pág. ${p - 1}: ${Math.round(f * 100)}% ocupada`), bar);
      }
    }
    m.onclick = () => { if (!PG.show) togglePages(true); pageScrollTo(p, 0); };
    anchor.before(m);
    last = p;
  }
}

// erros da compilação apontados no bloco
function markErrors() {
  document.querySelectorAll('#page .builderr').forEach(e => { e.classList.remove('builderr'); e.removeAttribute('data-err'); });
  const st = PG.status;
  if (!st || !st.failed || !st.errors) return;
  const src = S.blocks.map(b => b.src).join('');
  const lineStart = [0];
  for (let i = 0; i < src.length; i++) if (src[i] === '\n') lineStart.push(i + 1);
  for (const er of st.errors) {
    if (er.rel !== S.path || !er.line) continue;
    const pos = lineStart[er.line - 1] ?? 0;
    const b = S.blocks.find(x => x.start <= pos && pos < x.end);
    const t = b && editorElOf(b.id);
    if (t) { t.classList.add('builderr'); t.dataset.err = er.msg; }
  }
}

function showBuildInfo() {
  const st = PG.status || {};
  const box = el('div', 'buildinfo');
  if (st.main) box.append(el('div', '', `Arquivo principal compilado: ${st.main}`));
  if (st.tectonic === false) box.append(el('div', 'warn', 'O tectonic não está instalado.'));
  if (st.failed) {
    for (const er of st.errors || []) {
      const d = el('div', 'err-line');
      d.append(el('b', '', er.rel ? `${er.rel}:${er.line}` : 'erro'), document.createTextNode(' ' + er.msg));
      if (er.rel) {
        d.classList.add('link');
        d.onclick = async () => { $('#modal').hidden = true; if (er.rel !== S.path) await openFile(er.rel); markErrors(); const t = document.querySelector('#page .builderr'); if (t) t.scrollIntoView({ block: 'center' }); };
      }
      box.append(d);
    }
  }
  const sel = el('div', 'mainsel');
  box.append(sel);
  api('mains', { path: S.path }).then(r => {
    if (!r.mains || r.mains.length < 2) return;
    sel.append(el('span', '', 'Compilar com: '));
    const s = el('select');
    for (const m of r.mains) { const o = el('option', '', m); o.value = m; if (m === r.main) o.selected = true; s.append(o); }
    s.onchange = async () => { await api('setmain', { path: S.path, main: s.value }); PG.seq = 0; PG.pdfSeq = 0; scheduleBuild(10); };
    sel.append(s);
  }).catch(() => {});
  if (st.log) { const pre = el('pre', 'log', st.log); box.append(pre); }
  modal({ title: 'Compilação', readOnlyHtml: box });
}

function togglePages(on) {
  PG.show = on == null ? !PG.show : on;
  $('#pages').hidden = !PG.show;
  $('#b-pages').classList.toggle('on', PG.show);
  try { localStorage.setItem('tex-editor:pages', PG.show ? '1' : '0'); } catch (_) { /* sem storage */ }
  if (PG.show) { if (PG.pdf) buildPageShells(); else loadPdf(); }
}

// ------------------------------------------------------------ ganchos
// Compila só quando você manda (botão ▶ ou F5). O botão fica cinza quando o
// PDF já corresponde ao texto, e aceso quando há mudança para compilar.
PG.need = false;
function refreshPlay() {
  const b = $('#b-build');
  if (!b) return;
  const running = !!(PG.status && PG.status.running);
  b.disabled = running || !PG.need;
  b.classList.toggle('need', PG.need && !running);
  b.textContent = running ? 'Compilando…' : '▶ Compilar';
  b.title = running ? 'Compilando…' : PG.need ? 'Há mudanças: compilar agora (F5)' : 'O PDF já está atualizado';
}
function compileNow() {
  if (!S.path || (PG.status && PG.status.running)) return;
  commitDirty(false);
  enqueue(async () => { PG.need = false; refreshPlay(); startBuild(); });
}

hooks.open.push(() => {
  PG.layout = null; PG.seq = 0; PG.pdfSeq = 0; PG.rects = null; PG.focusIds = null; PG.lastFocusUnit = null;
  PG.keys = null; PG.keysSeq = null; PG.moved = [];
  $('#pages-list').textContent = '';
  $('#pages-title').textContent = 'Páginas';
  PG.keyPath = null;
  // usa a última compilação, se houver; senão o botão fica aceso
  setTimeout(async () => {
    try {
      const st = await api('buildstatus', { path: S.path });
      if (st.main != null) setBuildStatus(st);
      await loadLayout();
      PG.need = !(PG.layout && PG.layout.seq && !PG.layout.stale);
      if (st.running) pollBuild();
    } catch (_) { PG.need = true; }
    refreshPlay();
  }, 50);
});
let lastSrcKey = '';
hooks.state.push(r => {
  if (!r.blocks) return;
  let h = 2166136261;
  for (const b of r.blocks) for (let i = 0; i < b.src.length; i++) h = Math.imul(h ^ b.src.charCodeAt(i), 16777619);
  const key = r.blocks.length + ':' + (h >>> 0);
  if (PG.keyPath !== S.path) { PG.keyPath = S.path; lastSrcKey = key; return; }
  if (key !== lastSrcKey) {
    lastSrcKey = key;
    if (PG.layout) PG.layout.stale = true;
    PG.need = true;
    refreshPlay();
  }
});
hooks.render.push(() => { placeBands(); markErrors(); PG.lastFocusUnit = null; });

document.addEventListener('DOMContentLoaded', () => {
  $('#b-pages').onclick = () => togglePages();
  $('#build').onclick = showBuildInfo;
  $('#b-build').onclick = compileNow;
  document.addEventListener('keydown', e => { if (e.key === 'F5') { e.preventDefault(); compileNow(); } });
  let saved = '1';
  try { saved = localStorage.getItem('tex-editor:pages') ?? '1'; } catch (_) { /* sem storage */ }
  togglePages(saved === '1');
  let rt, gt, lastW = 0;
  window.addEventListener('resize', () => { clearTimeout(rt); rt = setTimeout(() => { if (PG.pdf && PG.show) buildPageShells(); }, 250); });
  // a página acompanha a largura disponível (janela, painéis abertos)
  new ResizeObserver(() => {
    const w = $('#doc').clientWidth;
    if (Math.abs(w - lastW) < 2) return;
    lastW = w;
    clearTimeout(gt);
    gt = setTimeout(() => { if (!PG.zoom && applyGeometry()) placeBands(); }, 120);
  }).observe($('#doc'));
  const cur = () => PG.zoom || Math.round(PG.px * 72 / 96 * 10) / 10;
  $('#z-in').onclick = () => setZoom(Math.min(3, cur() + 0.1));
  $('#z-out').onclick = () => setZoom(Math.max(0.5, cur() - 0.1));
  $('#z-fit').onclick = () => setZoom(0);
});
