'use strict';
// Diagramação: cada bloco (parágrafo, caixa, lista, imagem, linha de colunas)
// ganha uma alça ⠿ para arrastar e um menu. Soltar no meio da borda direita
// ou esquerda de um bloco põe os dois lado a lado (colunas).

const LY = { hover: null, drag: null, grip: null, ind: null, ghost: null, menu: null, imgh: null, imgTarget: null };

const isContainer = e => e && (e.id === 'page' || e.classList.contains('env-b') || e.classList.contains('list'));
const SKIP = NOT_UNIT;

// bloco "movível" mais interno que contém o nó
function unitOf(node) {
  let e = node && node.nodeType === 3 ? node.parentElement : node;
  if (e && e.closest && e.closest('.pgx, .pband')) return null;
  // figura flutuante desenhada onde o PDF a pôs: ela é o bloco
  const slot = e && e.closest && e.closest('#page .fslot');
  if (slot) return slot.firstElementChild;
  while (e && e.id !== 'page') {
    const p = e.parentElement;
    if (isContainer(p) && !SKIP.some(c => e.classList.contains(c))) return liftDecl(e);
    if (p && p.classList.contains('row') && isContainer(p.parentElement)) {
      // dentro de uma linha de colunas, a coluna não é movível sozinha; a linha é
      return liftDecl(p);
    }
    e = p;
  }
  return null;
}

const unitsIn = body => [...body.children].filter(c => !SKIP.some(k => c.classList.contains(k)));

// um bloco dentro de \begin{small}...\end{small} (sozinho) é o envoltório inteiro
function liftDecl(u) {
  for (let g = 0; u && g < 6; g++) {
    const body = u.parentElement;
    const w = body && body.classList.contains('env-b') ? body.parentElement : null;
    if (!w || !w.classList.contains('decl') || unitsIn(body).length !== 1) break;
    u = w;
  }
  return u;
}

// camadas de letra/alinhamento de um bloco: ['centering', 'small']
function declsOf(u) {
  const out = [];
  let w = u;
  while (w && w.classList && w.classList.contains('decl')) {
    out.push(w.dataset.decl);
    const kids = unitsIn(w.querySelector(':scope > .env-b'));
    w = kids.length === 1 ? kids[0] : null;
  }
  return out;
}

function blockById(id) { return S.blocks[+id]; }

// faixa de bytes [s, e) do bloco no fonte
function unitRange(u) {
  if (u.classList.contains('row')) {
    const cols = [...u.children].filter(c => c.dataset.begin != null);
    let s = blockById(cols[0].dataset.begin).start;
    let e = blockById(cols[cols.length - 1].dataset.end).end;
    const prev = u.previousElementSibling, next = u.nextElementSibling;
    if (prev && prev.classList.contains('glue')) s = blockById(prev.dataset.id).start;
    if (next && next.classList.contains('glue')) {
      const nb = blockById(next.dataset.id);
      if (nb.start === e) e = nb.end;
    }
    return { s, e };
  }
  if (u.dataset.begin != null && u.dataset.end != null) return { s: blockById(u.dataset.begin).start, e: blockById(u.dataset.end).end };
  const b = blockById(u.dataset.id);
  return b ? { s: b.start, e: b.end } : null;
}

const isItem = u => u.classList.contains('item');
const colOf = u => { const b = u.parentElement; return b && b.classList.contains('env-b') && b.parentElement.classList.contains('col') && b.parentElement.parentElement.classList.contains('row') ? b.parentElement : null; };
const rowRangeOf = col => unitRange(col.parentElement);

// o bloco de dentro das camadas de letra/alinhamento
function coreOf(u) {
  let w = u;
  while (w && w.classList.contains('decl')) {
    const k = unitsIn(w.querySelector(':scope > .env-b'));
    if (k.length !== 1) break;
    w = k[0];
  }
  return w;
}

function imageMacro(u) {
  u = coreOf(u);
  if (!u || !u.classList.contains('para')) return null;
  const m = u.querySelector(':scope > .macro[data-img]');
  return m && [...u.children].filter(c => !c.classList.contains("pgx")).length === 1 ? m : null;
}

// ---------------------------------------------------------- singletons
function setupSingletons() {
  LY.grip = el('div', 'grip', '⠿');
  LY.grip.title = 'Arraste para mover · clique para o menu';
  LY.grip.hidden = true;
  LY.ind = el('div', 'drop-ind');
  LY.ind.hidden = true;
  LY.ghost = el('div', 'drag-ghost');
  LY.ghost.hidden = true;
  LY.menu = el('div', 'umenu');
  LY.menu.hidden = true;
  LY.imgh = el('div', 'img-handle');
  LY.imgh.title = 'Arraste para mudar a largura da imagem';
  LY.imgh.hidden = true;
  LY.meas = el('div', 'measure');
  LY.meas.hidden = true;
  document.body.append(LY.grip, LY.ind, LY.ghost, LY.menu, LY.imgh, LY.meas);
  LY.grip.addEventListener('pointerdown', onGripDown);
  LY.imgh.addEventListener('pointerdown', onImgDown);
}

function placeGrip(u) {
  LY.hover = u;
  if (!u || LY.drag) { if (!LY.drag) LY.grip.hidden = true; return; }
  const r = u.getBoundingClientRect();
  const pr = $('#page').getBoundingClientRect();
  // colada à esquerda do bloco (na margem), não no canto da página
  LY.grip.style.left = Math.max(pr.left + 2, r.left - 28) + 'px';
  LY.grip.style.top = (r.top + 2) + 'px';
  LY.grip.hidden = false;
}

// bloco na mesma altura do ponteiro, quando ele está na margem da página
function unitBeside(e) {
  const page = $('#page');
  const pr = page.getBoundingClientRect();
  const cs = getComputedStyle(page);
  const left = pr.left + parseFloat(cs.paddingLeft), right = pr.right - parseFloat(cs.paddingRight);
  if (e.clientX >= left && e.clientX <= right) return null;
  LY.grip.style.pointerEvents = 'none';
  let n = document.elementFromPoint(left + 6, e.clientY);
  if (!unitOf(n)) n = document.elementFromPoint(Math.min(right - 6, left + 40), e.clientY);
  LY.grip.style.pointerEvents = '';
  return unitOf(n);
}

function onPageMove(e) {
  if (LY.drag || !$('#modal').hidden) return;
  let u = unitOf(e.target) || unitBeside(e);
  // a caminho da alça: continua no bloco enquanto o ponteiro estiver na faixa dele
  if (!u && LY.hover && document.contains(LY.hover)) {
    const r = LY.hover.getBoundingClientRect();
    if (e.clientY >= r.top - 4 && e.clientY <= r.bottom + 4 && e.clientX >= r.left - 60 && e.clientX <= r.right + 10) u = LY.hover;
  }
  if (u !== LY.hover) { placeGrip(u); if (u) hoverUnit(u); else showMeasure(null); }
  // imagem: alça de largura no canto
  const img = e.target.closest && e.target.closest('#page .macro[data-img] .mimg');
  if (img) placeImgHandle(img);
}

function hoverUnit(u) { paintHover(idsOfUnit(u)); showMeasure(u); }

// tamanho do bloco na página, em cm (como o inspetor do navegador)
function showMeasure(u) {
  const m = LY.meas;
  if (!m) return;
  if (!u || LY.drag || !document.body.classList.contains('pageview') || !document.contains(u)) { m.hidden = true; return; }
  const r = u.getBoundingClientRect();
  // as faixas de fim de página dentro do bloco não contam
  let bands = 0;
  u.querySelectorAll('.pband').forEach(b => { bands += b.getBoundingClientRect().height; });
  const k = 2.54 / 72 / PG.px;
  const f = v => (Math.round(v * 10) / 10).toFixed(1).replace('.', ',');
  m.textContent = `${f(r.width * k)} × ${f((r.height - bands) * k)} cm`;
  m.hidden = false;
  const dr = $('#doc').getBoundingClientRect();
  const top = r.top - 22 < dr.top ? r.top + 4 : r.top - 22;
  m.style.top = top + 'px';
  m.style.left = Math.max(dr.left + 4, r.right - m.offsetWidth) + 'px';
}

// ------------------------------------------------------------- arrastar
function onGripDown(e) {
  const u = LY.hover;
  if (!u) return;
  e.preventDefault();
  LY.grip.setPointerCapture(e.pointerId);
  LY.drag = { u, x: e.clientX, y: e.clientY, moving: false, target: null };
  const move = ev => onDragMove(ev);
  const up = ev => {
    LY.grip.removeEventListener('pointermove', move);
    LY.grip.removeEventListener('pointerup', up);
    const d = LY.drag;
    LY.drag = null;
    LY.ind.hidden = true; LY.ghost.hidden = true;
    document.body.classList.remove('dragging');
    if (d.u) d.u.classList.remove('drag-src');
    clearInterval(LY.scrollT); LY.scrollT = null;
    if (!d.moving) { select(d.u); return; }
    if (d.target) drop(d.u, d.target);
  };
  LY.grip.addEventListener('pointermove', move);
  LY.grip.addEventListener('pointerup', up);
}

function onDragMove(e) {
  const d = LY.drag;
  if (!d) return;
  if (!d.moving) {
    if (Math.hypot(e.clientX - d.x, e.clientY - d.y) < 5) return;
    d.moving = true;
    commitDirty(false);
    document.body.classList.add('dragging');
    d.u.classList.add('drag-src');
    LY.ghost.textContent = (d.u.textContent || '').trim().slice(0, 80) || 'bloco';
    LY.ghost.hidden = false;
    LY.menu.hidden = true;
  }
  LY.ghost.style.left = (e.clientX + 14) + 'px';
  LY.ghost.style.top = (e.clientY + 10) + 'px';
  // auto-rolagem perto das bordas
  // auto-rolagem: quanto mais perto da borda, mais rápido (para levar a outra página)
  const doc = $('#doc'), dr = doc.getBoundingClientRect();
  const edge = 70;
  d.lastX = e.clientX; d.lastY = e.clientY;
  d.v = e.clientY < dr.top + edge ? -(dr.top + edge - e.clientY) : e.clientY > dr.bottom - edge ? e.clientY - (dr.bottom - edge) : 0;
  if (d.v && !LY.scrollT) {
    LY.scrollT = setInterval(() => {
      const dd = LY.drag;
      if (!dd || !dd.v) { clearInterval(LY.scrollT); LY.scrollT = null; return; }
      doc.scrollTop += dd.v * 0.5;
      dd.target = findTarget(dd.lastX, dd.lastY, dd.u);
      showIndicator(dd.target);
    }, 16);
  }
  d.target = findTarget(e.clientX, e.clientY, d.u);
  showIndicator(d.target);
}

function findTarget(x, y, src) {
  LY.grip.style.pointerEvents = 'none';
  const n = document.elementFromPoint(x, y);
  LY.grip.style.pointerEvents = '';
  if (!n || !n.closest || !n.closest('#page')) return null;
  // espaço em branco no fim de uma página: o bloco vai para lá
  const band = n.closest('.pband');
  if (band) {
    if (band.classList.contains('pgx')) {
      const u = unitOf(band.parentElement);
      return u && u !== src && !src.contains(u) ? validTarget(src, { kind: 'before', t: u, band }) : null;
    }
    let t = band.nextElementSibling;
    while (t && SKIP.some(c => t.classList.contains(c))) t = t.nextElementSibling;
    if (t && t !== src) return validTarget(src, { kind: 'before', t, band });
    let p = band.previousElementSibling;
    while (p && SKIP.some(c => p.classList.contains(c))) p = p.previousElementSibling;
    if (p && p !== src) return validTarget(src, { kind: 'after', t: p, band });
    return null;
  }
  // coluna vazia: solta dentro dela
  const body = n.closest('.env-b, .list');
  let t = unitOf(n);
  if (!t && body && !body.querySelector(':scope > :not(.glue):not(.pagemark):not(.comment)')) {
    return validTarget(src, { kind: 'into', cont: body });
  }
  if (!t) return null;
  if (t === src || src.contains(t)) return null;
  const r = t.getBoundingClientRect();
  const side = r.width > 180 && !isItem(t) && !isItem(src) && !t.parentElement.classList.contains('list');
  if (side && x > r.right - r.width * 0.16) return validTarget(src, { kind: 'side', t, right: true });
  if (side && x < r.left + r.width * 0.10) return validTarget(src, { kind: 'side', t, right: false });
  return validTarget(src, { kind: y < r.top + r.height / 2 ? 'before' : 'after', t });
}

function validTarget(src, tg) {
  const cont = tg.cont || tg.t.parentElement;
  const inList = cont.classList.contains('list');
  if (isItem(src) !== inList && tg.kind !== 'side') return null;
  if (tg.t && tg.t.classList.contains('row') && tg.kind === 'side') {
    // ao lado de uma linha inteira não faz sentido: vai para antes/depois
    return null;
  }
  return tg;
}

function showIndicator(tg) {
  const ind = LY.ind;
  if (!tg) { ind.hidden = true; return; }
  ind.hidden = false;
  ind.className = 'drop-ind';
  const blank = tg.band && tg.band.querySelector('.pb-blank');
  if (blank && blank.offsetHeight > 6) {
    // destaca o espaço livre inteiro
    const r = blank.getBoundingClientRect();
    ind.classList.add('box');
    Object.assign(ind.style, { left: r.left + 'px', top: r.top + 'px', width: r.width + 'px', height: r.height + 'px' });
    return;
  }
  if (tg.kind === 'into') {
    const r = tg.cont.getBoundingClientRect();
    Object.assign(ind.style, { left: r.left + 'px', top: (r.top + 2) + 'px', width: r.width + 'px', height: '3px' });
    return;
  }
  const r = (colOf(tg.t) && tg.kind === 'side' ? colOf(tg.t) : tg.t).getBoundingClientRect();
  if (tg.kind === 'side') {
    ind.classList.add('v');
    Object.assign(ind.style, { left: (tg.right ? r.right - 2 : r.left - 4) + 'px', top: r.top + 'px', width: '4px', height: r.height + 'px' });
  } else {
    Object.assign(ind.style, { left: r.left + 'px', top: ((tg.kind === 'before' ? r.top : r.bottom) - 2) + 'px', width: r.width + 'px', height: '3px' });
  }
}

async function drop(u, tg) {
  await chain;
  const src = unitRange(u);
  if (!src) return;
  if (tg.kind === 'side') {
    const col = colOf(tg.t);
    const target = col ? { s: blockById(col.dataset.begin).start, e: blockById(col.dataset.end).end } : unitRange(tg.t);
    return op('side', { s: src.s, e: src.e, ts: target.s, te: target.e, right: tg.right });
  }
  let to;
  if (tg.kind === 'into') {
    const w = tg.cont.classList.contains('list') ? tg.cont : tg.cont.parentElement;
    to = blockById(w.dataset.begin).end;
  } else {
    const r = unitRange(tg.t);
    to = tg.kind === 'before' ? r.s : r.e;
  }
  return op('move', { s: src.s, e: src.e, to });
}

// ---------------------------------------------------------------- menu
function openMenu(u, ev) {
  const m = LY.menu;
  m.textContent = '';
  const add = (label, fn, cls) => {
    const b = el('button', 'mi' + (cls ? ' ' + cls : ''), label);
    b.onclick = () => { m.hidden = true; fn(); };
    m.append(b);
    return b;
  };
  const sep = () => m.append(el('div', 'msep'));
  const r = unitRange(u);
  const prev = siblingUnit(u, -1), next = siblingUnit(u, 1);
  const img = imageMacro(u);
  if (img) {
    const wrow = el('div', 'mwidths');
    wrow.append(el('span', 'ml', 'Largura'));
    const cur = parseFloat(img.dataset.w);
    for (const w of [0.3, 0.4, 0.5, 0.6, 0.75, 0.92, 1]) {
      const b = el('button', 'chip' + (Math.abs(cur - w) < 0.005 ? ' on' : ''), Math.round(w * 100) + '%');
      b.onclick = () => { m.hidden = true; setImageWidth(img, w); };
      wrow.append(b);
    }
    m.append(wrow);
    const meta = JSON.parse(img.dataset.meta);
    if (meta.name === '\\imagem' && S.macros['\\imagemH']) add('Fixar aqui (sem flutuar)', () => setImageMacro(img, '\\imagemH'));
    if (meta.name === '\\imagemH' && S.macros['\\imagem'] && !u.closest('.env-b')) add('Deixar flutuar (o LaTeX escolhe o lugar)', () => setImageMacro(img, '\\imagem'));
    sep();
  }
  if (prev) add('Mover para cima', () => op('move', { s: r.s, e: r.e, to: unitRange(prev).s }));
  if (next) add('Mover para baixo', () => op('move', { s: r.s, e: r.e, to: unitRange(next).e }));
  const col = colOf(u);
  const row = u.classList.contains('row') ? u : col && col.parentElement;
  if (next && !isItem(u) && !row && !next.classList.contains('row') && !isItem(next)) {
    add('Lado a lado com o próximo', () => op('side', { s: unitRange(next).s, e: unitRange(next).e, ts: r.s, te: r.e, right: true }));
  }
  if (row) {
    sep();
    const rr = { get s() { return unitRange(row).s; }, get e() { return unitRange(row).e; } };
    add('Adicionar coluna', () => op('addcol', { pos: rr.s }));
    add('Colunas iguais', () => {
      const n = row.querySelectorAll(':scope > .col').length;
      const total = 1 - 0.04 * (n - 1);
      op('widths', { pos: rr.s, widths: Array(n).fill(Math.floor(total / n * 100) / 100) });
    });
    add('Desfazer colunas', () => op('unrow', { pos: rr.s }));
  }
  sep();
  add('Quebra de página antes', () => op('pagebreak', { pos: r.s, cmd: 'newpage' }));
  add('Duplicar', () => op('dupunit', { s: r.s, e: r.e }));
  add('Editar código', () => editUnitSource(u));
  add('Excluir', () => { if (confirm('Excluir este bloco?')) op('delunit', { s: r.s, e: r.e }); }, 'danger');
  m.hidden = false;
  const x = Math.min(ev.clientX + 6, innerWidth - 250), y = Math.min(ev.clientY, innerHeight - m.offsetHeight - 8);
  m.style.left = x + 'px';
  m.style.top = Math.max(8, y) + 'px';
}

function siblingUnit(u, dir) {
  let s = dir < 0 ? u.previousElementSibling : u.nextElementSibling;
  while (s && SKIP.some(c => s.classList.contains(c))) s = dir < 0 ? s.previousElementSibling : s.nextElementSibling;
  return s;
}

function editUnitSource(u) {
  const r = unitRange(u);
  const src = S.blocks.map(b => b.src).join('');
  modal({
    title: 'Código do bloco',
    src: src.slice(r.s, r.e).replace(/\n$/, ''),
    onOk: v => op('replace', { s: r.s, e: r.e, text: v }),
  });
}

// ------------------------------------------------------------- imagens
async function ensureWidthOption(name) {
  const M = S.macros[name];
  if (!M || M.opt) return true;
  if (!confirm(`Para cada imagem ter a sua largura, o \\${name.slice(1)} precisa aceitar a largura como argumento opcional.\n\nO editor vai mudar a definição em ${M.file} (as imagens que já existem continuam com ${Math.round(M.default * 100)}%). Dá para desfazer.`)) return false;
  await commitDirty(false);
  await chain;
  const r = await api('imgopt', { path: S.path, name });
  S.macros = r.macros;
  const other = name === '\\imagem' ? '\\imagemH' : '\\imagem';
  if (S.macros[other] && !S.macros[other].opt) {
    const r2 = await api('imgopt', { path: S.path, name: other });
    S.macros = r2.macros;
  }
  refreshDirtyFiles();
  toast(`${r.file} alterado: \\imagem agora aceita largura. Salve para gravar.`);
  return true;
}

async function setImageWidth(mac, w) {
  const meta = JSON.parse(mac.dataset.meta);
  if (!(await ensureWidthOption(meta.name))) return;
  w = Math.round(Math.max(0.1, Math.min(1, w)) * 100) / 100;
  // os nós do DOM podem ter sido redesenhados: reencontra pelo bloco
  const blk = mac.closest('.blk');
  const id = blk && blk.dataset.id;
  const live = id != null ? $(`#page .blk[data-id="${id}"] > .macro[data-img]`) : mac;
  if (!live) return;
  const m2 = JSON.parse(live.dataset.meta);
  if (m2.args[0] && m2.args[0].opt) m2.args[0].raw = String(w);
  else m2.args.unshift({ opt: true, raw: String(w) });
  live.dataset.meta = JSON.stringify(m2);
  structuralEdit(live.closest('.blk'));
}

function setImageMacro(mac, name) {
  const meta = JSON.parse(mac.dataset.meta);
  meta.name = name;
  mac.dataset.meta = JSON.stringify(meta);
  structuralEdit(mac.closest('.blk'));
}

function placeImgHandle(img) {
  LY.imgTarget = img;
  const r = img.getBoundingClientRect();
  LY.imgh.style.left = (r.right - 9) + 'px';
  LY.imgh.style.top = (r.bottom - 9) + 'px';
  LY.imgh.hidden = false;
}

function onImgDown(e) {
  const img = LY.imgTarget;
  if (!img) return;
  e.preventDefault();
  const mac = img.closest('.macro');
  const box = mac.getBoundingClientRect();
  const label = el('div', 'img-w');
  mac.append(label);
  LY.imgh.setPointerCapture(e.pointerId);
  let w = parseFloat(mac.dataset.w) || 1;
  const move = ev => {
    // largura relativa à coluna de texto (centralizada)
    const half = Math.abs(ev.clientX - (box.left + box.width / 2));
    w = Math.max(0.1, Math.min(1, (half * 2) / box.width));
    w = Math.round(w * 50) / 50;
    img.style.width = (w * 100) + '%';
    label.textContent = Math.round(w * 100) + '%';
    LY.imgh.hidden = true;
  };
  const up = () => {
    LY.imgh.removeEventListener('pointermove', move);
    LY.imgh.removeEventListener('pointerup', up);
    label.remove();
    if (Math.abs(w - parseFloat(mac.dataset.w)) > 0.005) setImageWidth(mac, w);
  };
  LY.imgh.addEventListener('pointermove', move);
  LY.imgh.addEventListener('pointerup', up);
}

// ------------------------------------------------- largura das colunas
function addColumnHandles() {
  for (const row of document.querySelectorAll('#page .row')) {
    const cols = [...row.querySelectorAll(':scope > .col')];
    cols.slice(0, -1).forEach((c, i) => {
      const h = el('div', 'col-handle');
      h.title = 'Arraste para mudar a largura das colunas';
      h.contentEditable = 'false';
      c.append(h);
      h.addEventListener('pointerdown', ev => startColResize(ev, row, cols, i));
    });
  }
}

function startColResize(e, row, cols, i) {
  e.preventDefault();
  e.stopPropagation();
  const h = e.currentTarget;
  h.setPointerCapture(e.pointerId);
  const rw = row.getBoundingClientRect().width;
  const ws = cols.map(c => parseFloat(c.dataset.width) || 0.48);
  const x0 = e.clientX;
  const a0 = ws[i], b0 = ws[i + 1];
  const tip = el('div', 'img-w');
  row.append(tip);
  const move = ev => {
    const d = (ev.clientX - x0) / rw;
    const a = Math.max(0.1, Math.min(a0 + b0 - 0.1, a0 + d));
    ws[i] = Math.round(a * 100) / 100;
    ws[i + 1] = Math.round((a0 + b0 - a) * 100) / 100;
    cols[i].style.flex = `0 0 ${ws[i] * 100}%`;
    cols[i + 1].style.flex = `0 0 ${ws[i + 1] * 100}%`;
    tip.textContent = `${Math.round(ws[i] * 100)}% | ${Math.round(ws[i + 1] * 100)}%`;
  };
  const up = () => {
    h.removeEventListener('pointermove', move);
    h.removeEventListener('pointerup', up);
    tip.remove();
    if (ws[i] !== a0) op('widths', { pos: unitRange(row).s, widths: ws });
  };
  h.addEventListener('pointermove', move);
  h.addEventListener('pointerup', up);
}

// ---------------------------------------------- seleção e propriedades
// O bloco selecionado (clique, alça ⠿ ou cursor no texto) aparece na barra
// de propriedades: letra, alinhamento, largura, espaços, posição, colunas.

const SIZE_ORDER = ['tiny', 'scriptsize', 'footnotesize', 'small', 'normalsize', 'large', 'Large', 'LARGE'];
const ALIGN_ORDER = [['raggedright', 'À esquerda'], ['centering', 'Centralizado'], ['raggedleft', 'À direita'], ['justify', 'Justificado (normal)']];
const ICON = {
  raggedright: 'M2 3h12M2 6.5h8M2 10h12M2 13.5h7',
  centering: 'M2 3h12M4 6.5h8M2 10h12M4.5 13.5h7',
  raggedleft: 'M2 3h12M6 6.5h8M2 10h12M7 13.5h7',
  justify: 'M2 3h12M2 6.5h12M2 10h12M2 13.5h12',
};
const svgIcon = d => `<svg viewBox="0 0 16 16" width="16" height="16"><path d="${d}" stroke="currentColor" stroke-width="1.6" fill="none"/></svg>`;

function select(u) {
  if (u && !document.contains(u)) u = null;
  if (u === LY.sel) { paintProps(); return; }
  document.querySelectorAll('#page .sel').forEach(x => x.classList.remove('sel'));
  LY.sel = u;
  LY.selKey = u ? (unitRange(u) || {}).s : null;
  if (u) { u.classList.add('sel'); onEditorFocusUnit(u); }
  paintProps();
}

// depois de redesenhar: o mesmo bloco (pela posição no fonte)
function reselect() {
  if (LY.sel && document.contains(LY.sel)) { paintProps(); return; }
  LY.sel = null;
  if (LY.selKey != null) {
    for (const e of document.querySelectorAll('#page [data-id], #page [data-begin]')) {
      const u = unitOf(e);
      if (u && (unitRange(u) || {}).s === LY.selKey) { select(u); return; }
    }
  }
  paintProps();
}

function unitInfo(u) {
  const c = coreOf(u);
  if (u.classList.contains('row')) return `Colunas (${u.querySelectorAll(':scope > .col').length})`;
  if (c.classList.contains('vsp')) return 'Espaço vertical';
  if (imageMacro(u)) return 'Imagem';
  if (c.classList.contains('list')) return 'Lista';
  if (c.classList.contains('env')) { const st = S.styles[c.dataset.env] || {}; return 'Caixa: ' + (st.title || pretty(c.dataset.env)); }
  if (c.classList.contains('heading')) return 'Título';
  if (c.classList.contains('item')) return 'Item de lista';
  if (c.classList.contains('raw')) { const b = S.blocks[+c.dataset.id]; return b && b.table ? 'Tabela' : b && b.env ? 'Ambiente ' + b.env : 'Código'; }
  if (c.classList.contains('cmd')) return 'Comando';
  return 'Parágrafo';
}

function pagesOf(u) {
  const rs = rectsOf(idsOfUnit(u));
  return rs.length ? [rs[0][0], rs[rs.length - 1][0]] : null;
}

// espaço vertical logo antes/depois do bloco
function spacerNear(u, dir) {
  let s = dir < 0 ? u.previousElementSibling : u.nextElementSibling;
  while (s && ['pband', 'glue', 'comment', 'pv-top'].some(c => s.classList.contains(c))) s = dir < 0 ? s.previousElementSibling : s.nextElementSibling;
  return s && s.classList.contains('vsp') ? s : null;
}

// leva o bloco para depois do primeiro bloco da página seguinte (ou para
// antes do último da anterior), no mesmo nível
function toPage(u, dir) {
  const pg = pagesOf(u);
  if (!pg) return;
  let s = siblingUnit(u, dir);
  while (s) {
    const p = pagesOf(s);
    if (p && (dir > 0 ? p[0] > pg[0] : p[1] < pg[0])) break;
    s = siblingUnit(s, dir);
  }
  if (!s) { toast(dir > 0 ? 'Não há bloco na página seguinte neste nível.' : 'Não há bloco na página anterior neste nível.'); return; }
  const r = unitRange(u), t = unitRange(s);
  op('move', { s: r.s, e: r.e, to: dir > 0 ? t.e : t.s });
}

function paintProps() {
  const bar = $('#props');
  if (!bar) return;
  if (bar.contains(document.activeElement) && document.activeElement.type === 'range') return; // arrastando um controle
  bar.textContent = '';
  const u = LY.sel && document.contains(LY.sel) ? LY.sel : null;
  if (!u) {
    bar.append(el('span', 'pp-hint', 'Clique num bloco (ou na alça ⠿ à esquerda dele) para mudar a diagramação: letra, largura, espaços, colunas, página. Arraste a alça para mover o bloco, inclusive para o espaço livre no fim de uma página.'));
    return;
  }
  const r = unitRange(u);
  if (!r) return;
  const R = () => unitRange(u);
  const c = coreOf(u);
  const img = imageMacro(u);
  const col = u.classList.contains('row') ? null : colOf(u);
  const row = u.classList.contains('row') ? u : col && col.parentElement;
  const item = isItem(c);
  // imagem sozinha numa coluna (alinhada à esquerda ou à direita)
  const cols1 = row === u ? row.querySelectorAll(':scope > .col') : [];
  const one = cols1.length === 1 ? unitsIn(cols1[0].querySelector(':scope > .env-b')) : null;
  const rimg = one && one.length === 1 ? imageMacro(one[0]) : null;
  const prev = siblingUnit(u, -1), next = siblingUnit(u, 1);
  const group = label => { const g = el('div', 'pp-g'); if (label) g.append(el('span', 'pp-l', label)); bar.append(g); return g; };
  const btn = (g, text, title, fn, on) => {
    const b = el('button', 'pb' + (on ? ' on' : ''), text);
    b.title = title;
    b.onclick = () => flushThen(fn);
    g.append(b);
    return b;
  };
  const slider = (g, val, min, max, live, done) => {
    const i = el('input', 'pp-range');
    i.type = 'range'; i.min = min; i.max = max; i.value = val;
    const v = el('span', 'pp-v', val + '%');
    i.oninput = () => { v.textContent = i.value + '%'; live(+i.value); };
    i.onchange = () => { i.blur(); flushThen(() => done(+i.value)); };
    g.append(i, v);
  };

  const head = group('');
  head.classList.add('pp-head');
  head.append(el('b', '', unitInfo(u)));
  const pg = pagesOf(u);
  const fl = c.classList.contains('floatp') || u.classList.contains('floatp');
  if (pg) head.append(el('span', 'pp-page', fl ? `flutua · pág. ${pg[0]}` : pg[0] === pg[1] ? `pág. ${pg[0]}` : `págs. ${pg[0]}–${pg[1]}`));
  if (PG.layout && PG.layout.stale) head.append(el('span', 'pp-stale', 'recompilando…'));

  if (c.classList.contains('vsp')) {
    const g = group('Altura');
    const l = +c.dataset.lines;
    btn(g, '−', 'Diminuir ¼ de linha', () => setSpacer(c, l - 0.25));
    g.append(el('span', 'pp-v', linesLabel(l)));
    btn(g, '+', 'Aumentar ¼ de linha', () => setSpacer(c, l + 0.25));
  } else if (!img && !rimg) {
    // letra
    const ds = declsOf(u);
    const size = ds.find(x => DECL_SIZE[x]) || 'normalsize';
    const k = SIZE_ORDER.indexOf(size);
    const fs = geom().fs;
    const setSize = v => op('decl', { ...R(), kind: 'size', value: v });
    const g = group('Letra');
    btn(g, 'A−', 'Letra menor neste bloco', () => setSize(SIZE_ORDER[k - 1])).disabled = k <= 0;
    const sel = el('select', 'pp-sel');
    for (const z of SIZE_ORDER) {
      const o = el('option', '', `${(Math.round(DECL_SIZE[z] * fs * 10) / 10).toString().replace('.', ',')} pt${z === 'normalsize' ? ' (normal)' : ''}`);
      o.value = z;
      if (z === size) o.selected = true;
      sel.append(o);
    }
    sel.title = 'Tamanho da letra do bloco inteiro (\\begin{small}...)';
    sel.onchange = () => flushThen(() => setSize(sel.value));
    g.append(sel);
    btn(g, 'A+', 'Letra maior neste bloco', () => setSize(SIZE_ORDER[k + 1])).disabled = k >= SIZE_ORDER.length - 1;
    // alinhamento
    const al = ds.find(x => DECL_ALIGN[x]) || 'justify';
    const ga = group('Alinhar');
    for (const [v, t] of ALIGN_ORDER) btn(ga, '', t, () => op('decl', { ...R(), kind: 'align', value: v }), al === v).innerHTML = svgIcon(ICON[v]);
  }

  // imagem à esquerda, ao centro ou à direita
  const anyImg = img || rimg;
  if (anyImg) {
    const ga = group('Imagem');
    const cur = rimg ? (row.dataset.align === 'right' ? 'right' : 'left') : 'center';
    const meta = JSON.parse(anyImg.dataset.meta);
    const M = S.macros || {};
    const fixed = M[meta.name + 'H'] ? meta.name + 'H' : meta.name;
    const place = async side => {
      if (!(await ensureWidthOption(meta.name))) return;
      op('imgalign', { ...R(), side, name: meta.name, fixed, default: (M[meta.name] || {}).default || 0.92 });
    };
    for (const [v, t, ic] of [['left', 'Imagem à esquerda', 'raggedright'], ['center', 'Imagem centralizada', 'centering'], ['right', 'Imagem à direita', 'raggedleft']]) {
      btn(ga, '', t, () => place(v), cur === v).innerHTML = svgIcon(ICON[ic]);
    }
  }

  // largura
  if (img) {
    const g = group('Largura');
    const pic = img.querySelector('.mimg');
    slider(g, Math.round(parseFloat(img.dataset.w) * 100), 10, 100, v => { if (pic) pic.style.width = v + '%'; }, v => setImageWidth(img, v / 100));
    const meta = JSON.parse(img.dataset.meta);
    const M = S.macros || {};
    if (meta.name === '\\imagem' && M['\\imagemH']) btn(g, 'Fixar aqui', 'A imagem fica exatamente neste lugar (\\imagemH)', () => setImageMacro(img, '\\imagemH'));
    if (meta.name === '\\imagemH' && M['\\imagem'] && !u.closest('.env-b')) btn(g, 'Deixar flutuar', 'O LaTeX escolhe o lugar da imagem (\\imagem)', () => setImageMacro(img, '\\imagem'));
  } else if (row && row === u && row.querySelectorAll(':scope > .col').length === 1) {
    const g = group('Largura');
    const cc = row.querySelector(':scope > .col');
    slider(g, Math.round(parseFloat(cc.dataset.width) * 100), 20, 100, v => { cc.style.flex = `0 0 ${v}%`; }, v => op('blockwidth', { ...R(), w: v / 100 }));
  } else if (!row && !item && !c.classList.contains('vsp') && !c.classList.contains('heading')) {
    const g = group('Largura');
    slider(g, 100, 20, 100, v => { u.style.width = v + '%'; }, v => op('blockwidth', { ...R(), w: v / 100 }));
  }

  // espaços antes e depois
  if (!c.classList.contains('vsp')) {
    const gs = group('Espaço');
    for (const [side, dir, label] of [['before', -1, 'antes'], ['after', 1, 'depois']]) {
      const v = spacerNear(u, dir);
      const cur = v ? +v.dataset.lines : 0;
      gs.append(el('span', 'pp-sub', label));
      btn(gs, '−', `Menos espaço ${label} (½ linha; negativo aperta)`, () => op('vspace', { ...R(), side, lines: Math.max(-3, cur - 0.5) }));
      gs.append(el('span', 'pp-v', cur ? (cur > 0 ? '+' : '') + (Math.round(cur * 100) / 100 + '').replace('.', ',') : '0'));
      btn(gs, '+', `Mais espaço ${label} (½ linha)`, () => op('vspace', { ...R(), side, lines: cur + 0.5 }));
    }
  }

  // posição
  const gp = group('Posição');
  btn(gp, '↑', 'Subir um bloco (Alt+↑)', () => op('move', { ...R(), to: unitRange(prev).s })).disabled = !prev;
  btn(gp, '↓', 'Descer um bloco (Alt+↓)', () => op('move', { ...R(), to: unitRange(next).e })).disabled = !next;
  btn(gp, '◂ pág.', 'Levar para a página anterior', () => toPage(u, -1)).disabled = !pg || fl;
  btn(gp, 'pág. ▸', 'Levar para a próxima página', () => toPage(u, 1)).disabled = !pg || fl;
  btn(gp, 'Nova página antes', 'Começa uma página nova antes deste bloco (\\newpage)', () => op('pagebreak', { pos: R().s, cmd: 'newpage' }));

  // colunas
  const gc = group('Colunas');
  if (row) {
    const rr = { get s() { return unitRange(row).s; }, get e() { return unitRange(row).e; } };
    const n = row.querySelectorAll(':scope > .col').length;
    if (n === 2) btn(gc, '⇄ Trocar lados', 'Inverte as colunas (ex.: texto à esquerda, imagem à direita)', () => op('swapcols', { pos: rr.s }));
    btn(gc, '+ Coluna', 'Adiciona uma coluna nesta linha', () => op('addcol', { pos: rr.s }));
    if (n > 1) btn(gc, 'Iguais', 'Colunas da mesma largura', () => {
      const total = 1 - 0.04 * (n - 1);
      op('widths', { pos: rr.s, widths: Array(n).fill(Math.floor(total / n * 100) / 100) });
    });
    if (col) btn(gc, 'Tirar da coluna', 'Põe este bloco embaixo das colunas', () => op('move', { ...R(), to: rr.e }));
    btn(gc, 'Desfazer colunas', 'Os blocos voltam a ficar um embaixo do outro', () => op('unrow', { pos: rr.s }));
  } else if (!item) {
    const ok = x => x && !isItem(x) && !x.classList.contains('row');
    if (ok(next)) btn(gc, '▥ Com o próximo', 'Este bloco e o seguinte lado a lado (este à esquerda)', () => op('side', { s: unitRange(next).s, e: unitRange(next).e, ts: R().s, te: R().e, right: true }));
    if (ok(prev)) btn(gc, '▥ Com o anterior', 'O bloco anterior e este lado a lado (este à direita)', () => op('side', { ...R(), ts: unitRange(prev).s, te: unitRange(prev).e, right: true }));
  }

  const gx = group('');
  gx.classList.add('pp-end');
  btn(gx, 'Duplicar', 'Duplicar o bloco', () => op('dupunit', { ...R() }));
  btn(gx, 'Código', 'Editar o LaTeX deste bloco', () => editUnitSource(u));
  btn(gx, 'Excluir', 'Excluir o bloco (Ctrl+Z desfaz)', () => op('delunit', { ...R() })).classList.add('danger');
}

// altura de um espaço vertical: arrastar a borda de baixo
function onVspDown(e) {
  const h = e.target.closest && e.target.closest('#page .vsp-h');
  if (!h) return;
  e.preventDefault();
  e.stopPropagation();
  const sp = h.parentElement;
  h.setPointerCapture(e.pointerId);
  const y0 = e.clientY, l0 = +sp.dataset.lines;
  const lh = parseFloat(getComputedStyle($('#page')).lineHeight) || 20;
  const lab = sp.querySelector('.vsp-l');
  let l = l0;
  const move = ev => {
    l = Math.max(0, Math.round((Math.max(0, l0) + (ev.clientY - y0) / lh) * 4) / 4);
    sp.style.height = `calc(${l} * var(--bs))`;
    lab.textContent = '↕ ' + linesLabel(l);
  };
  const up = () => {
    h.removeEventListener('pointermove', move);
    h.removeEventListener('pointerup', up);
    if (l !== l0) setSpacer(sp, l);
  };
  h.addEventListener('pointermove', move);
  h.addEventListener('pointerup', up);
}

function onSelKey(e) {
  const u = LY.sel;
  if (!u || !document.contains(u) || !$('#modal').hidden) return;
  if (e.altKey && (e.key === 'ArrowUp' || e.key === 'ArrowDown')) {
    e.preventDefault();
    const dir = e.key === 'ArrowUp' ? -1 : 1;
    flushThen(() => {
      const t = siblingUnit(u, dir), r = unitRange(u);
      if (t && r) op('move', { s: r.s, e: r.e, to: dir < 0 ? unitRange(t).s : unitRange(t).e });
    });
    return;
  }
  // bloco sem texto selecionado (imagem, tabela, espaço, caixa): Delete exclui
  const typing = e.target.closest && e.target.closest('[contenteditable="true"], input, textarea, select');
  if (e.key === 'Delete' && !typing) {
    e.preventDefault();
    const r = unitRange(u);
    if (r) op('delunit', { s: r.s, e: r.e });
  }
}

// ------------------------------------------------------------ ganchos
hooks.render.push(() => { addColumnHandles(); LY.hover = null; if (LY.grip) LY.grip.hidden = true; if (LY.imgh) LY.imgh.hidden = true; reselect(); });
hooks.op.push(r => {
  let t = r.focus && $(`#page [data-id="${r.focus.id}"]`);
  // o cursor caiu na "cola" antes de uma linha de colunas: o bloco é o seguinte
  while (t && !unitOf(t) && t.nextElementSibling) t = t.nextElementSibling;
  const u = t && unitOf(t);
  if (u) select(u);
});

document.addEventListener('DOMContentLoaded', () => {
  setupSingletons();
  const page = $('#page');
  page.addEventListener('mousemove', onPageMove);
  $('#doc').addEventListener('scroll', () => { if (LY.hover && !LY.drag) { placeGrip(LY.hover); showMeasure(LY.hover); } LY.imgh.hidden = true; });
  page.addEventListener('focusin', e => { const u = unitOf(e.target); if (u) select(u); });
  page.addEventListener('pointerdown', e => {
    if (e.target.closest('.vsp-h')) return onVspDown(e);
    if (e.target.closest('.col-handle, .pband')) return;
    select(unitOf(e.target));
  });
  document.addEventListener('keydown', onSelKey);
  LY.grip.addEventListener('contextmenu', e => { e.preventDefault(); if (LY.hover) openMenu(LY.hover, e); });
  paintProps();
  document.addEventListener('pointerdown', e => { if (!LY.menu.hidden && !LY.menu.contains(e.target) && e.target !== LY.grip) LY.menu.hidden = true; });
  document.addEventListener('keydown', e => { if (e.key === 'Escape') LY.menu.hidden = true; });
  $('#doc').addEventListener('mouseleave', e => {
    const to = e.relatedTarget;
    if (LY.drag || to === LY.grip || to === LY.imgh || (to && LY.menu.contains(to))) return;
    LY.grip.hidden = true;
    LY.meas.hidden = true;
    paintHover(null);
  });
  LY.grip.addEventListener('mouseenter', () => { if (LY.hover) LY.hover.classList.add('unit-hover'); });
  LY.grip.addEventListener('mouseleave', () => { document.querySelectorAll('.unit-hover').forEach(x => x.classList.remove('unit-hover')); });
});
