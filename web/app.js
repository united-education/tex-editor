'use strict';
// TeX Editor: o navegador só desenha e lê o modelo; quem gera o .tex é o Rust.

const $ = (s, r = document) => r.querySelector(s);
const TOKEN = $('meta[name=token]').content;
const LISTS = ['itemize', 'enumerate', 'description', 'compactitem', 'compactenum'];
const KMACROS = {
  '\\celsius': '{^\\circ\\mathrm{C}}', '\\percent': '\\%', '\\degree': '^\\circ',
  '\\SI': '#1\\,\\mathrm{#2}', '\\qty': '#1\\,\\mathrm{#2}', '\\si': '\\mathrm{#1}',
  '\\num': '#1', '\\unit': '\\mathrm{#1}', '\\kilo': 'k', '\\gram': 'g', '\\metre': 'm',
  '\\meter': 'm', '\\second': 's', '\\litre': 'L', '\\liter': 'L', '\\mole': 'mol',
  '\\milli': 'm', '\\micro': '\\mu ', '\\per': '/', '\\joule': 'J', '\\kelvin': 'K',
};

const S = {
  path: null, blocks: [], styles: {}, files: [], dirtyFiles: new Set(),
  dirtyId: null, timer: null, ver: 0, busy: false, buffer: '', lastId: null,
  showSource: false, srcEditing: false,
};
let chain = Promise.resolve();

// ------------------------------------------------------------------ util
async function api(name, body = {}) {
  const r = await fetch('/api/' + name, {
    method: 'POST',
    headers: { 'x-token': TOKEN, 'content-type': 'application/json' },
    body: JSON.stringify(body),
  });
  const j = await r.json().catch(() => ({ error: r.statusText }));
  if (!r.ok || j.error) { const e = new Error(j.error || r.statusText); e.status = r.status; throw e; }
  return j;
}
function enqueue(fn) {
  chain = chain.then(fn).catch(err => toast(err.message, true));
  return chain;
}
let toastT;
function toast(msg, err) {
  const t = $('#toast');
  t.textContent = msg; t.className = err ? 'err' : ''; t.hidden = false;
  clearTimeout(toastT); toastT = setTimeout(() => (t.hidden = true), err ? 6000 : 2200);
}
function el(tag, cls, text) {
  const e = document.createElement(tag);
  if (cls) e.className = cls;
  if (text != null) e.textContent = text;
  return e;
}
function esc(s) { return s.replace(/[&<>]/g, c => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;' }[c])); }
function pretty(name) { return name.replace(/\*$/, '').replace(/^./, c => c.toUpperCase()); }

function braceArgs(src) {
  const out = []; let d = 0, start = -1;
  for (let i = 0; i < src.length; i++) {
    const c = src[i];
    if (c === '\\') { i++; continue; }
    if (c === '{') { if (d === 0) start = i + 1; d++; }
    else if (c === '}') { d--; if (d === 0) out.push(src.slice(start, i)); }
  }
  return out;
}

function katexInto(node, tex, display) {
  if (!window.katex) { node.textContent = tex; return; }
  try {
    katex.render(tex, node, { displayMode: display, throwOnError: true, strict: 'ignore', macros: { ...KMACROS } });
  } catch (e) {
    node.textContent = tex; node.classList.add('err'); node.title = e.message;
  }
}
function stripMath(src) {
  if (src.startsWith('$$')) return src.slice(2, -2);
  if (src.startsWith('$')) return src.slice(1, -1);
  if (src.startsWith('\\(') || src.startsWith('\\[')) return src.slice(2, -2);
  return src;
}
const UNITS = {
  celsius: '°C', percent: '%', degree: '°', kilo: 'k', gram: 'g', metre: 'm', meter: 'm',
  second: 's', litre: 'L', liter: 'L', mole: 'mol', milli: 'm', micro: 'µ', per: '/',
  squared: '²', cubed: '³', joule: 'J', kelvin: 'K', hour: 'h', minute: 'min', calorie: 'cal',
  watt: 'W', volt: 'V', ampere: 'A', newton: 'N', pascal: 'Pa', hertz: 'Hz', mega: 'M', giga: 'G',
  centi: 'c', nano: 'n', ohm: 'Ω', tonne: 't', day: 'd',
};
function unitText(u) {
  // \, \; \: \! são espaços finos: no PDF não aparecem como texto
  return u.replace(/\\[,;:! ]/g, ' ').replace(/\\([a-zA-Z]+)/g, (m, n) => UNITS[n] ?? n).replace(/[{}~]/g, '').replace(/[ \t\n]+/g, '');
}
function siText(src) {
  const name = (src.match(/^\\([a-zA-Z]+)/) || [])[1];
  const a = braceArgs(src);
  if (name === 'si' || name === 'unit') return unitText(a[0] || '');
  if (name === 'num') return a[0] || '';
  if (name === 'ang') return (a[0] || '') + '°';
  if (name === 'SIrange' || name === 'numrange') return `${a[0]}–${a[1]} ${unitText(a[2] || '')}`;
  const u = unitText(a[1] || '');
  return (a[0] || '') + (u === '%' || u === '°' ? ' ' : ' ') + u;
}

// ------------------------------------------------------------- desenho
function renderInl(nodes, parent) {
  for (const n of nodes || []) parent.append(renderNode(n));
}
function renderNode(n) {
  switch (n.t) {
    case 'text': return document.createTextNode(n.v);
    case 'fmt': {
      const tag = { textbf: 'b', textit: 'i', emph: 'i', textsl: 'i', underline: 'u', uline: 'u', textsuperscript: 'sup', textsubscript: 'sub', texttt: 'code' }[n.cmd] || 'span';
      const e = el(tag, { textsc: 'sc', textsf: 'sf' }[n.cmd] || '');
      e.dataset.cmd = n.cmd;
      renderInl(n.c, e);
      return e;
    }
    case 'group': {
      const e = el('span', 'grp');
      const first = (n.c || []).find(x => x.t !== 'text' || x.v.trim());
      if (first && first.t === 'atom') {
        const m = first.src.match(/^\\(small|footnotesize|large|Large)\b/);
        if (m) e.classList.add('sz-' + m[1]);
      }
      renderInl(n.c, e);
      return e;
    }
    case 'macro': return renderMacro(n);
    case 'atom': return renderAtom(n);
  }
  return document.createTextNode('');
}
// número da figura ("1.1") pelo \label da macro e o .aux da última compilação
function figKey(n, M) {
  if (!M.label) return '';
  const vals = n.args.map(a => a.raw != null ? a.raw : '');
  if (M.labelopt && !(n.args[0] && n.args[0].opt)) vals.unshift(String(M.default));
  return M.label.replace(/#(\d)/g, (_, d) => (vals[+d - 1] || '').trim());
}
function figNumber(key) {
  const labels = typeof PG !== 'undefined' && PG.layout && PG.layout.labels;
  return (labels && labels[key]) || '?';
}

function renderMacro(n) {
  const e = el('span', 'macro');
  e.contentEditable = 'false';
  // cada argumento de texto guarda o índice do seu campo (data-ai), que não muda
  // mesmo que outro argumento entre antes (ex.: a largura opcional de uma imagem)
  e.dataset.meta = JSON.stringify({ name: n.name, args: n.args.map((a, k) => (a.c ? { text: true, ai: k } : { opt: a.opt, raw: a.raw })) });
  const label = el('span', 'mname', n.name.slice(1));
  e.append(label);
  // comando do preâmbulo que abre parágrafo próprio (\estrategia, \dica...): desenhado como no PDF
  const look = (S.styles || {})[n.name];
  if (look && look.block) {
    e.classList.add('mblock');
    if (look.small) e.style.fontSize = '0.913em';
    if (look.footnote) e.style.fontSize = '0.822em';
    if (look.italic) e.style.fontStyle = 'italic';
    if (look.color) e.style.color = look.color;
    if (look.vbefore) e.style.marginTop = `calc(${look.vbefore} * var(--pt, 1.33px))`;
    if (look.vafter) e.style.marginBottom = `calc(${look.vafter} * var(--pt, 1.33px))`;
    if (look.chip) {
      const c = el('span', 'mchip', look.chip);
      if (look.chipbg) c.style.background = look.chipbg;
      if (look.chipfg) c.style.color = look.chipfg;
      e.append(c);
    }
  }
  const M = (S.macros || {})[n.name];
  const mand = n.args.filter(a => !a.opt);
  const fileArg = M && M.id != null ? mand[M.id] : n.args.find(a => !a.opt && a.raw != null);
  if (fileArg && fileArg.raw != null && (M || /imag|fig|graphic/i.test(n.name)) && /^[\w./-]+$/.test(fileArg.raw.trim())) {
    const img = el('img', 'mimg');
    img.src = `/api/img?doc=${encodeURIComponent(S.path)}&name=${encodeURIComponent(fileArg.raw.trim())}`;
    img.onerror = () => {
      // sem arquivo: o PDF mostra o marcador com a descrição, então ela volta a aparecer
      e.querySelectorAll('.marg.mhide').forEach(x => x.classList.remove('mhide'));
      img.replaceWith(el('div', 'mimg missing', '[imagem ' + fileArg.raw.trim() + ']'));
    };
    img.draggable = false;
    if (M) {
      const opt = n.args[0] && n.args[0].opt && M.opt ? parseFloat(n.args[0].raw) : NaN;
      const w = isNaN(opt) ? M.default : opt;
      img.style.width = (w * 100).toFixed(1) + '%';
      e.dataset.img = '1';
      e.dataset.w = w;
    }
    e.append(img);
  }
  if (M && M.centered) e.classList.add('mcenter');
  let mi = -1;
  n.args.forEach((a, k) => {
    if (!a.opt) mi++;
    if (a.c) {
      const s = el('span', 'marg');
      s.contentEditable = 'true';
      s.dataset.ai = k;
      if (M && M.hidden && M.hidden.includes(mi) && e.querySelector('img.mimg')) {
        // só sai no PDF quando a imagem falta: não aparece
        s.classList.add('mhide');
      }
      // como o argumento sai no PDF: {\small #3}, \caption{#3}
      const sty = M && M.argsty && M.argsty[mi];
      if (sty) {
        if (sty.size) s.style.fontSize = DECL_SIZE[sty.size] + 'em';
        if (sty.italic) s.style.fontStyle = 'italic';
        if (sty.caption) {
          s.classList.add('mcap');
          const key = figKey(n, M);
          const pre = el('span', 'noread capname', `Figura ${figNumber(key)}: `);
          pre.dataset.key = key;
          pre.contentEditable = 'false';
          // como o caption: centralizada se cabe numa linha, senão justificada
          const cw = el('span', 'capwrap');
          cw.append(pre, s);
          renderInl(a.c, s);
          e.append(cw);
          return;
        } else if (M.centered) s.classList.add('mcentered');
      }
      renderInl(a.c, s);
      e.append(s);
    } else {
      const r = el('span', 'mraw', a.opt ? `[${a.raw}]` : a.raw);
      r.title = 'Clique para editar o código';
      e.append(r);
    }
  });
  return e;
}
function renderAtom(a) {
  const s = el('span', 'atom k-' + a.kind);
  s.contentEditable = 'false';
  s.dataset.src = a.src;
  s.dataset.kind = a.kind;
  if (a.d != null) s.dataset.d = a.d;
  paintAtom(s);
  return s;
}
function paintAtom(s) {
  const src = s.dataset.src, kind = s.dataset.kind;
  s.textContent = ''; s.classList.remove('err'); s.title = src.length < 300 ? src : '';
  switch (kind) {
    case 'math': katexInto(s, stripMath(src), false); break;
    case 'dmath': katexInto(s, stripMath(src), true); break;
    case 'ce': katexInto(s, src, false); break;
    case 'si': s.textContent = siText(src); break;
    case 'sym': s.textContent = s.dataset.d; break;
    case 'br': s.append('↵', document.createElement('br')); break;
    case 'ref': {
      const m = src.match(/^\\([a-zA-Z]+)/); const a = braceArgs(src);
      s.textContent = m && m[1].startsWith('cite') ? `[${a[0]}]` : (a[a.length - 1] || src);
      break;
    }
    case 'img': {
      const a = braceArgs(src);
      const img = el('img'); img.src = `/api/img?doc=${encodeURIComponent(S.path)}&name=${encodeURIComponent((a[0] || '').trim())}`;
      img.onerror = () => { s.textContent = src; };
      s.append(img); break;
    }
    case 'env': s.textContent = src.split('\n')[0].slice(0, 60) + (src.length > 60 ? '…' : ''); break;
    case 'cmd': {
      // \resolucao (sem argumento, abre parágrafo com etiqueta): como no PDF
      const nm = (src.match(/^\\[a-zA-Z]+/) || [''])[0];
      const look = (S.styles || {})[nm];
      if (look && look.block) {
        s.classList.add('ablock');
        const c = el('span', 'mchip', look.chip || nm.slice(1));
        if (look.chipbg) c.style.background = look.chipbg;
        if (look.chipfg) c.style.color = look.chipfg;
        if (look.small) s.style.fontSize = '0.913em';
        s.append(c);
      } else s.textContent = src.trim();
      break;
    }
    default: s.textContent = src;
  }
}

function listMarker(list, n) {
  const env = list.dataset.env, opt = list.dataset.opt || '';
  if (env === 'itemize' || env === 'compactitem') return list.dataset.depth % 2 ? '–' : '•';
  if (env === 'description') return '';
  const m = opt.match(/label\s*=\s*\{?([^,\]}]*\\(Alph|alph|roman|Roman|arabic)\*[^,\]}]*)\}?/);
  let style = 'arabic', pre = '', post = '.';
  if (m) {
    style = m[2];
    const parts = m[1].split(/\\(?:Alph|alph|roman|Roman|arabic)\*/);
    pre = parts[0]; post = parts[1] || '';
  }
  const roman = k => { const v = [[10, 'x'], [9, 'ix'], [5, 'v'], [4, 'iv'], [1, 'i']]; let r = ''; for (const [a, s] of v) while (k >= a) { r += s; k -= a; } return r; };
  const f = { Alph: String.fromCharCode(64 + n), alph: String.fromCharCode(96 + n), roman: roman(n), Roman: roman(n).toUpperCase(), arabic: String(n) }[style];
  return pre + f + post;
}

function textBlock(b) {
  const tag = b.kind === 'heading' ? 'h' + Math.min(Math.max(b.level, 1), 5) : b.kind === 'item' ? 'div' : 'p';
  const e = el(tag, 'blk ' + b.kind);
  e.dataset.id = b.id;
  if (b.locked) {
    e.classList.add('locked');
    e.textContent = b.src;
    e.title = 'Este trecho não pôde ser interpretado com segurança. Clique para editar o código.';
    return e;
  }
  if (b.kind === 'heading' && b.env.endsWith('*')) e.classList.add('star');
  if (b.kind === 'heading') {
    e.dataset.h = b.env.replace(/\*$/, '');
    // \titleformat do preâmbulo: tamanho e cor do título como no PDF
    const tf = (S.styles || {})['#' + b.env.replace(/\*$/, '')];
    if (tf) {
      e.classList.add('tf');
      if (tf.size) e.style.setProperty('--hsize', { Huge: 2.272, huge: 1.894, LARGE: 1.578, Large: 1.315, large: 1.096, normalsize: 1, small: 0.913 }[tf.size] + 'em');
      if (tf.color) e.style.color = tf.color;
      if (tf.bold === false) e.style.fontWeight = '400';
    }
  }
  // \noindent no começo: sem recuo, como no PDF
  const first = (b.inl || []).find(n => n.t !== 'text' || n.v.trim());
  if (first && first.t === 'atom' && /^\\noindent\b/.test(first.src)) e.classList.add('noind');
  e.contentEditable = 'true';
  renderInl(b.inl, e);
  if (b.inl && b.inl.length === 1 && b.inl[0].t === 'macro') e.firstChild.classList.add('solo');
  return e;
}

// corpo 11 do LaTeX: \small = 10 pt, \footnotesize = 9 pt...
const DECL_SIZE = { tiny: 0.548, scriptsize: 0.73, footnotesize: 0.822, small: 0.913, normalsize: 1, large: 1.096, Large: 1.315, LARGE: 1.578 };
const DECL_ALIGN = { raggedright: 'left', centering: 'center', raggedleft: 'right' };

function envBox(b) {
  if (LISTS.includes(b.env)) {
    const l = el('div', 'list ' + b.env);
    l.dataset.env = b.env; l.dataset.opt = b.opt || ''; l.dataset.count = 0; l.dataset.begin = b.id;
    // espaçamento da lista (pt), como o enumitem/LaTeX: corpo 11 -> topsep 9, itemsep+parsep 9
    const o = b.opt || '';
    let top = 9, gap = 9;
    if (/\bnosep\b/.test(o)) { top = 0; gap = 0; } else if (/\bnoitemsep\b/.test(o)) gap = 0;
    const is = o.match(/itemsep\s*=\s*([\d.]+)pt/);
    if (is) gap = parseFloat(is[1]) + (/\bnosep\b/.test(o) ? 0 : 4.5);
    const ts = o.match(/topsep\s*=\s*([\d.]+)pt/);
    if (ts) top = parseFloat(ts[1]);
    l.style.setProperty('--topsep', top);
    l.style.setProperty('--itemgap', gap);
    return { wrap: l, body: l };
  }
  const st = S.styles[b.env] || {};
  const w = el('div', 'env');
  w.dataset.env = b.env;
  w.dataset.begin = b.id;
  if (b.env === 'minipage') {
    const m = b.src.match(/\{\s*([0-9.]*)\s*\\(?:linewidth|textwidth|columnwidth|hsize)\s*\}/);
    w.dataset.width = m ? (m[1] ? parseFloat(m[1]) : 1) : 1;
    w.classList.add('col');
  }
  if (st.colframe) w.style.setProperty('--frame', st.colframe);
  if (st.colback) w.style.setProperty('--bg-env', st.colback);
  if (st.colbacktitle) w.style.setProperty('--tbg', st.colbacktitle);
  if (st.coltitle) w.style.setProperty('--tcol', st.coltitle);
  if (st.bar) w.classList.add('bar');
  if (st.tcb) {
    // medidas do tcolorbox (pt), para o modo página desenhar a caixa igual ao PDF
    w.classList.add('tcb');
    const rule = st.boxrule ?? 1.42;
    const vars = {
      '--trule': st.toprule ?? rule, '--brule': st.bottomrule ?? rule, '--lrule': st.leftrule ?? rule, '--rrule': st.rightrule ?? rule,
      '--arc': st.arc ?? 2.85, '--boxsep': st.boxsep ?? 2.85, '--pl': st.left ?? 11.38, '--pr': st.right ?? 11.38,
      '--ptop': st.top ?? 5.69, '--pbot': st.bottom ?? 5.69, '--ttop': st.toptitle ?? 0, '--tbot': st.bottomtitle ?? 0,
      '--bskip': st.before_skip ?? 4.1, '--askip': st.after_skip ?? 4.1,
    };
    for (const [k, v] of Object.entries(vars)) w.style.setProperty(k, v);
    if (st.titlesize) w.style.setProperty('--tsize', DECL_SIZE[st.titlesize] + 'em');
  }
  if (st.italic) w.classList.add('italic');
  if (st.small) w.classList.add('small');
  const known = Object.keys(st).length > 0;
  if (!known && (DECL_SIZE[b.env] || DECL_ALIGN[b.env])) {
    // \begin{small}, \begin{centering}...: só muda a letra ou o alinhamento do que está dentro
    w.classList.add('plain', 'decl');
    w.dataset.decl = b.env;
  }
  if (!known && ['document', 'center', 'flushleft', 'flushright', 'minipage', 'multicols', 'multicols*', 'small', 'footnotesize', 'quote', 'quotation', 'adjustwidth'].includes(b.env)) {
    w.classList.add('plain');
    if (b.env === 'center') w.classList.add('center');
    if (b.env === 'quote' || b.env === 'quotation') w.classList.add('quote');
  }
  const h = el('div', 'env-h');
  h.contentEditable = 'false';
  const title = st.title || pretty(b.env);
  h.append(el('span', '', title + (b.opt ? ' — ' + b.opt : '')));
  h.append(el('span', 'envname', b.env));
  h.dataset.id = b.id;
  h.title = 'Clique para editar o cabeçalho do ambiente';
  const body = el('div', 'env-b');
  if (w.dataset.decl) {
    if (DECL_SIZE[b.env]) body.style.fontSize = DECL_SIZE[b.env] + 'em';
    if (DECL_ALIGN[b.env]) { body.style.textAlign = DECL_ALIGN[b.env]; body.classList.add('noindent', 'aligned'); }
  }
  w.append(h, body);
  return { wrap: w, body };
}

function rawBlock(b) {
  const w = el('div', 'raw ' + b.kind);
  w.dataset.id = b.id;
  w.title = 'Clique para editar o código';
  if (b.kind === 'math') {
    const m = el('div');
    const src = b.src.trim();
    katexInto(m, src.startsWith('\\[') || src.startsWith('$$') ? stripMath(src) : src, true);
    w.append(m);
    return w;
  }
  if (b.target && !b.table) {
    const img = el('img');
    img.src = `/api/img?doc=${encodeURIComponent(S.path)}&name=${encodeURIComponent(b.target.trim())}`;
    img.onerror = () => img.remove();
    w.append(img);
  }
  if (b.table) {
    const t = el('table');
    for (const row of b.table) {
      const tr = el('tr');
      for (const c of row) {
        const td = el('td');
        if (c.span > 1) td.colSpan = c.span;
        renderInl(c.inl, td);
        td.querySelectorAll('[contenteditable]').forEach(x => x.removeAttribute('contenteditable'));
        tr.append(td);
      }
      t.append(tr);
    }
    w.append(t);
    w.append(el('span', 'tag', b.env || 'tabela'));
    return w;
  }
  w.append(el('pre', '', b.src.replace(/\n+$/, '')));
  if (b.env) w.append(el('span', 'tag', b.env));
  return w;
}

function renderDoc(focus) {
  const page = $('#page');
  const doc = $('#doc');
  const top = doc.scrollTop;
  page.textContent = '';
  const stack = [page];
  for (const b of S.blocks) {
    const cur = stack[stack.length - 1];
    switch (b.kind) {
      case 'blank': break;
      case 'comment': { const c = el('div', 'comment', b.src.replace(/\n$/, '')); c.dataset.id = b.id; cur.append(c); break; }
      case 'para': case 'heading': case 'item': {
        if (b.kind === 'para' && isGlue(b) && spaceOf(b.src)) { cur.append(spacerBlock(b)); break; }
        if (b.kind === 'para' && isGlue(b)) {
          // só \par, \noindent, \hfill...: invisível (aparece no modo ¶)
          const g = el('div', 'glue', b.src.trim());
          g.dataset.id = b.id;
          cur.append(g);
          break;
        }
        const e = textBlock(b);
        if (b.kind === 'item' && cur.classList.contains('list')) {
          cur.dataset.count = +cur.dataset.count + 1;
          cur.dataset.depth = stack.filter(x => x.classList && x.classList.contains('list')).length - 1;
          e.dataset.marker = b.opt != null ? b.opt : listMarker(cur, +cur.dataset.count);
        }
        cur.append(e);
        break;
      }
      case 'begin': { const { wrap, body } = envBox(b); cur.append(wrap); stack.push(body); break; }
      case 'end': if (stack.length > 1) {
        const body = stack.pop();
        const w = body.classList.contains('list') ? body : body.parentElement;
        w.dataset.end = b.id;
      } break;
      case 'raw': case 'math': cur.append(rawBlock(b)); break;
      case 'cmd': {
        const c = el('div', 'cmd');
        c.dataset.id = b.id;
        if (/^\\(newpage|clearpage|cleardoublepage|pagebreak)\b/.test(b.src.trim())) {
          c.classList.add('pbreak');
          c.append(el('span', 'pb-label', 'Quebra de página'));
        }
        c.append(el('span', '', b.src.trim().split('\n')[0]));
        if (b.target) {
          const o = el('button', 'tb wide open', 'Abrir ' + b.target.split('/').pop());
          o.onclick = ev => { ev.stopPropagation(); openFile(b.target); };
          c.append(o);
        }
        cur.append(c);
        break;
      }
    }
  }
  if (!S.blocks.length) page.append(el('div', 'empty-doc', 'Arquivo vazio.'));
  groupRows(page);
  doc.scrollTop = top;
  hooks.render.forEach(f => f());
  if (focus) setFocus(focus);
  renderSource();
}

// minipages seguidas (com \hfill, \par\noindent entre elas) viram uma linha de colunas
function groupRows(root) {
  const isMini = e => e && e.classList && e.classList.contains('env') && e.dataset.env === 'minipage';
  const containers = [root, ...root.querySelectorAll('.env-b, .list')];
  for (const c of containers) {
    let kids = [...c.children];
    for (let i = 0; i < kids.length; i++) {
      if (!isMini(kids[i])) continue;
      const run = [kids[i]];
      let j = i + 1;
      while (j < kids.length) {
        if (kids[j].classList.contains('glue')) { j++; continue; }
        if (isMini(kids[j])) { run.push(kids[j]); j++; continue; }
        break;
      }
      const row = el('div', 'row');
      // \par\noindent\hfill antes: a linha (de uma coluna) fica à direita
      const pv = kids[i].previousElementSibling;
      if (pv && pv.classList.contains('glue') && pv.textContent.includes('\\hfill')) row.dataset.align = 'right';
      kids[i].before(row);
      // cola antes e depois da linha fica de fora (invisível); as colunas entram
      for (const m of run) {
        const w = (m.dataset.width && parseFloat(m.dataset.width)) || 1;
        m.style.flex = `0 0 ${(w * 100).toFixed(1)}%`;
        row.append(m);
      }
      kids = [...c.children];
      i = kids.indexOf(row);
    }
  }
}

// Espaço vertical (\vspace, \medskip...): desenhado com a altura que terá no PDF.
const SKIP_PT = { smallskip: 3, medskip: 6, bigskip: 12 };
function geom() { return (typeof PG !== 'undefined' && PG.layout && PG.layout.geom) || { fs: 10.95, bs: 13.6 }; }
function spaceOf(src) {
  const t = src.trim();
  let m = t.match(/^\\(small|med|big)skip$/);
  if (m) return { lines: SKIP_PT[m[1] + 'skip'] / geom().bs, std: true };
  m = t.match(/^\\vspace\*?\{\s*(-?[\d.]*)\s*(\\baselineskip|cm|mm|pt|bp|em|ex|in)\s*\}$/);
  if (!m) return null;
  const v = m[1] === '' ? 1 : m[1] === '-' ? -1 : parseFloat(m[1]);
  // \vspace{0pt} (topo das colunas) não é espaço: fica invisível
  if (isNaN(v) || v === 0) return null;
  if (m[2] === '\\baselineskip') return { lines: v };
  const g = geom();
  const pt = v * { cm: 28.4528, mm: 2.84528, pt: 1, bp: 1.00375, in: 72.27, em: g.fs, ex: g.fs * 0.43 }[m[2]];
  return { lines: pt / g.bs };
}
function linesLabel(l) {
  const r = Math.round(l * 100) / 100;
  return (r + '').replace('.', ',') + (Math.abs(r) === 1 ? ' linha' : ' linhas');
}
function spacerBlock(b) {
  const sp = spaceOf(b.src);
  const e = el('div', 'vsp');
  e.dataset.id = b.id;
  e.dataset.lines = sp.lines;
  e.contentEditable = 'false';
  e.style.height = `calc(${Math.max(0, sp.lines).toFixed(3)} * var(--bs))`;
  if (sp.lines < 0) e.classList.add('neg');
  e.title = 'Espaço vertical (' + b.src.trim() + '). Arraste a borda de baixo para mudar; clique para selecionar.';
  e.append(el('span', 'vsp-l', (sp.lines < 0 ? 'aperta ' : '↕ ') + linesLabel(Math.abs(sp.lines))), el('div', 'vsp-h'));
  return e;
}

function isGlue(b) {
  return (b.inl || []).length > 0 && b.inl.every(n => (n.t === 'text' && !n.v.trim()) || (n.t === 'atom' && n.kind === 'layout'));
}

const hooks = { render: [], state: [], open: [], op: [] };

// Letra proporcional à página: a mancha do livro tem ~41 "em" de largura
// (16 cm em corpo 11), então as linhas quebram parecido com o PDF.
function fitFont() {
  const page = $('#page');
  if (!page || document.body.classList.contains('pageview')) return; // modo página: medidas do PDF
  const cs = getComputedStyle(page);
  const w = page.clientWidth - parseFloat(cs.paddingLeft) - parseFloat(cs.paddingRight);
  const px = Math.max(12, Math.min(19, w / 41));
  page.style.fontSize = px.toFixed(2) + 'px';
}
window.addEventListener('resize', fitFont);
document.addEventListener('DOMContentLoaded', () => { fitFont(); new ResizeObserver(fitFont).observe($('#doc')); });

function replaceBlock(id) {
  const old = $(`#page .blk[data-id="${id}"]`);
  const b = S.blocks[id];
  if (!old || !b || !b.inl) return;
  const had = old.contains(document.activeElement);
  const off = had ? getFlat(old) : null;
  const e = textBlock(b);
  if (old.dataset.marker) e.dataset.marker = old.dataset.marker;
  old.replaceWith(e);
  if (had && off != null) setFlat(e, off);
}

// -------------------------------------------- cursor (offset "plano")
function acceptText(n, blk) {
  const host = n.parentElement && n.parentElement.closest('[contenteditable]');
  if (!host || host.getAttribute('contenteditable') !== 'true' || !blk.contains(host)) return false;
  // figura flutuante desenhada (só na tela) dentro deste parágrafo: não é dele
  const slot = n.parentElement.closest('.fslot');
  return !(slot && blk.contains(slot));
}
function textNodes(blk) {
  const w = document.createTreeWalker(blk, NodeFilter.SHOW_TEXT, {
    acceptNode: n => (acceptText(n, blk) ? NodeFilter.FILTER_ACCEPT : NodeFilter.FILTER_REJECT),
  });
  const out = []; let n;
  while ((n = w.nextNode())) out.push(n);
  return out;
}
function getFlat(blk) {
  const sel = getSelection();
  if (!sel.rangeCount) return null;
  const r = sel.getRangeAt(0);
  if (!blk.contains(r.startContainer)) return null;
  let sum = 0;
  const pt = document.createRange();
  pt.setStart(r.startContainer, r.startOffset);
  for (const t of textNodes(blk)) {
    if (t === r.startContainer) return sum + r.startOffset;
    if (pt.comparePoint(t, 0) < 0) sum += t.length; else return sum;
  }
  return sum;
}
function flatLen(blk) { return textNodes(blk).reduce((a, t) => a + t.length, 0); }
function setFlat(blk, off) {
  const sel = getSelection();
  const r = document.createRange();
  let placed = false;
  for (const t of textNodes(blk)) {
    if (off <= t.length) { r.setStart(t, off); placed = true; break; }
    off -= t.length;
  }
  if (!placed) { r.selectNodeContents(blk); r.collapse(false); }
  r.collapse(true);
  const host = (r.startContainer.nodeType === 3 ? r.startContainer.parentElement : r.startContainer).closest('[contenteditable="true"]') || blk;
  host.focus({ preventScroll: true });
  sel.removeAllRanges();
  sel.addRange(r);
  const rect = (r.getClientRects()[0] || blk.getBoundingClientRect());
  const d = $('#doc').getBoundingClientRect();
  if (rect.top < d.top + 40 || rect.bottom > d.bottom - 40) blk.scrollIntoView({ block: 'center' });
}
function setFocus(f) {
  if (!f) return;
  const blk = $(`#page .blk[data-id="${f.id}"]`);
  if (blk) { setFlat(blk, f.offset); S.lastId = f.id; syncStyleSelect(); }
}
function curBlock() {
  const sel = getSelection();
  if (!sel.rangeCount) return null;
  const n = sel.getRangeAt(0).startContainer;
  const e = (n.nodeType === 3 ? n.parentElement : n);
  return e && e.closest ? e.closest('#page .blk') : null;
}

// --------------------------------------------------- leitura do DOM
function readInl(node, out = []) {
  const push = n => {
    const last = out[out.length - 1];
    if (n.t === 'text' && last && last.t === 'text') last.v += n.v; else out.push(n);
  };
  for (const c of node.childNodes) {
    if (c.nodeType === 3) { if (c.data) push({ t: 'text', v: c.data }); continue; }
    if (c.nodeType !== 1) continue;
    if (c.classList.contains('caretmark')) { out.push({ t: 'caret' }); continue; }
    if (c.classList.contains('pgx') || c.classList.contains('noread')) continue; // desenho do editor, não é texto
    if (c.classList.contains('parmark')) { out.push({ t: 'par' }); continue; }
    if (c.classList.contains('atom')) { out.push({ t: 'atom', src: c.dataset.src }); continue; }
    if (c.classList.contains('macro')) {
      const meta = JSON.parse(c.dataset.meta);
      const args = meta.args.map((a, k) => {
        if (!a.text) return { opt: !!a.opt, raw: a.raw };
        const ai = a.ai ?? k;
        const s = c.querySelector(`:scope > .marg[data-ai="${ai}"], :scope > .capwrap > .marg[data-ai="${ai}"]`);
        return { opt: false, c: s ? readInl(s, []) : [] };
      });
      out.push({ t: 'macro', name: meta.name, args });
      continue;
    }
    if (c.tagName === 'BR') continue;
    let cmd = c.dataset.cmd;
    if (!cmd) cmd = { B: 'textbf', STRONG: 'textbf', I: 'emph', EM: 'emph', U: 'underline', SUP: 'textsuperscript', SUB: 'textsubscript' }[c.tagName];
    if (cmd) {
      const inner = readInl(c, []);
      if (inner.some(x => x.t !== 'caret')) out.push({ t: 'fmt', cmd, c: inner });
      else out.push(...inner);
      continue;
    }
    if (c.classList.contains('grp')) { out.push({ t: 'group', c: readInl(c, []) }); continue; }
    if (c.tagName === 'DIV' || c.tagName === 'P') { if (out.length) push({ t: 'par' }); }
    readInl(c, out);
  }
  return out;
}
function readBlock(blk, withCaret) {
  let mark = null, off = null;
  if (withCaret) {
    const sel = getSelection();
    if (sel.rangeCount && blk.contains(sel.getRangeAt(0).startContainer)) {
      off = getFlat(blk);
      const r = sel.getRangeAt(0).cloneRange();
      r.collapse(true);
      mark = el('span', 'caretmark');
      r.insertNode(mark);
    }
  }
  const inl = readInl(blk, []);
  if (mark) {
    const p = mark.parentNode; mark.remove(); p.normalize();
    if (off != null) setFlat(blk, off);
  }
  return inl;
}

// ------------------------------------------------------------ envio
function applyState(r) {
  if (r.blocks) S.blocks = r.blocks;
  setDirty(r.dirty);
  $('#b-undo').disabled = !r.can_undo;
  $('#b-redo').disabled = !r.can_redo;
  hooks.state.forEach(f => f(r));
}
function setDirty(d) {
  $('#dirty').classList.toggle('on', !!d);
  if (d) S.dirtyFiles.add(S.path); else S.dirtyFiles.delete(S.path);
  const f = $(`#tree .f[data-path="${CSS.escape(S.path || '')}"]`);
  if (f) f.classList.toggle('dirty', !!d);
  $('#status').textContent = d ? 'Não salvo' : 'Salvo';
}

function markDirty(blk) {
  const id = blk.dataset.id;
  S.ver++;
  if (S.dirtyId != null && S.dirtyId !== id) commitDirty(false);
  S.dirtyId = id;
  clearTimeout(S.timer);
  S.timer = setTimeout(() => commitDirty(false), 650);
  $('#dirty').classList.add('on');
}

function withBusy(fn) {
  return enqueue(async () => {
    S.busy = true;
    try { await fn(); } finally {
      S.busy = false;
      if (S.buffer) {
        const t = S.buffer; S.buffer = '';
        document.execCommand('insertText', false, t);
      }
    }
  });
}

// Grava o bloco sujo. structural => redesenha e posiciona o cursor.
function commitDirty(structural) {
  clearTimeout(S.timer);
  const id = S.dirtyId;
  S.dirtyId = null;
  if (id == null) return chain;
  const blk = $(`#page [data-id="${id}"]`);
  if (!blk) return chain;
  if (blk.dataset.phantom) return commitPhantom(blk);
  const inl = readBlock(blk, structural);
  const ver = S.ver;
  const send = async () => {
    const r = await api('edit', { path: S.path, id: +id, inl, structural });
    applyState(r);
    if (structural || r.changed) renderDoc(r.focus);
    else {
      if (!r.same && ver === S.ver) replaceBlock(+id);
      renderSource();
    }
  };
  return structural ? withBusy(send) : enqueue(send);
}

function structuralEdit(blk) {
  S.dirtyId = blk.dataset.id;
  return commitDirty(true);
}

function commitPhantom(blk) {
  const inl = readBlock(blk, true);
  return withBusy(async () => {
    const r = await api('insert', { path: S.path, id: +blk.dataset.anchor, where: blk.dataset.where, inl });
    applyState(r);
    renderDoc(r.focus);
  });
}

function makePhantom(anchor, where) {
  const isItem = anchor.classList.contains('item');
  const p = el(isItem ? 'div' : 'p', 'blk phantom ' + (isItem ? 'item' : 'para'));
  p.contentEditable = 'true';
  p.dataset.phantom = '1';
  p.dataset.anchor = anchor.dataset.id;
  p.dataset.where = where;
  p.dataset.id = 'ph' + Date.now();
  if (isItem) p.dataset.marker = anchor.dataset.marker && /^[•–]$/.test(anchor.dataset.marker) ? anchor.dataset.marker : '…';
  if (where === 'after') anchor.after(p); else anchor.before(p);
  setFlat(p, 0);
}

function op(name, body, then) {
  return withBusy(async () => {
    const r = await api(name, { path: S.path, ...body });
    applyState(r);
    renderDoc(r.focus);
    hooks.op.forEach(f => f(r));
    if (then) then(r);
  });
}

// Enter numa linha vazia: \vspace{N\baselineskip} no lugar dela
function phantomToSpace(ph) {
  const id = +ph.dataset.anchor, where = ph.dataset.where;
  const anchor = $(`#page [data-id="${id}"]`);
  ph.remove();
  const b = S.blocks[id];
  if (!anchor || !b) return;
  const after = r => {
    const sp = r.focus && $(`#page .vsp[data-id="${r.focus.id}"]`);
    if (sp) makePhantom(sp, 'after');
  };
  if (anchor.classList.contains('vsp')) {
    setSpacer(anchor, +anchor.dataset.lines + 1, after);
    return;
  }
  const sib = where === 'after' ? anchor.nextElementSibling : anchor.previousElementSibling;
  const cur = sib && sib.classList.contains('vsp') ? +sib.dataset.lines : 0;
  op('vspace', { s: b.start, e: b.end, side: where, lines: cur + 1 }, after);
}

function setSpacer(sp, lines, then) {
  const b = S.blocks[+sp.dataset.id];
  if (!b) return;
  lines = Math.round(lines * 4) / 4;
  if (Math.abs(lines) < 0.01) return op('delunit', { s: b.start, e: b.end }, then);
  const v = Math.round(lines * 100) / 100;
  return op('replace', { s: b.start, e: b.end, text: `\\vspace{${v === 1 ? '' : v}\\baselineskip}` }, then);
}

// ------------------------------------------------------- teclado etc.
function hoistedRange(blk) {
  const sel = getSelection();
  let r = sel.getRangeAt(0).cloneRange();
  r.collapse(true);
  let e = r.startContainer.nodeType === 3 ? r.startContainer.parentElement : r.startContainer;
  while (e && e !== blk && blk.contains(e) && !e.classList.contains('marg')) {
    const pre = document.createRange();
    pre.selectNodeContents(e);
    pre.setEnd(r.startContainer, r.startOffset);
    const before = pre.toString().length;
    const total = e.textContent.length;
    const nr = document.createRange();
    if (before === total) nr.setStartAfter(e);
    else if (before === 0) nr.setStartBefore(e);
    else break;
    nr.collapse(true);
    r = nr;
    e = e.parentElement;
  }
  return r;
}

// Coloca o cursor logo depois da (primeira) quebra de parágrafo do modelo.
function insertCaretAfterPar(nodes) {
  for (let i = 0; i < nodes.length; i++) {
    const n = nodes[i];
    if (n.t === 'par') { nodes.splice(i + 1, 0, { t: 'caret' }); return true; }
    if (n.c && insertCaretAfterPar(n.c)) return true;
  }
  return false;
}

function navBlocks() { return [...document.querySelectorAll('#page .blk[contenteditable="true"]')]; }

function moveTo(blk, dir, x) {
  const list = navBlocks();
  const i = list.indexOf(blk);
  const t = list[i + dir];
  if (!t) return false;
  if (x != null) {
    const rc = t.getBoundingClientRect();
    const y = dir > 0 ? rc.top + 8 : rc.bottom - 8;
    t.scrollIntoView({ block: 'nearest' });
    const rc2 = t.getBoundingClientRect();
    const yy = dir > 0 ? rc2.top + 8 : rc2.bottom - 8;
    const pos = document.caretRangeFromPoint(Math.min(Math.max(x, rc2.left + 1), rc2.right - 1), yy);
    if (pos && t.contains(pos.startContainer)) {
      const host = (pos.startContainer.nodeType === 3 ? pos.startContainer.parentElement : pos.startContainer).closest('[contenteditable="true"]') || t;
      host.focus({ preventScroll: true });
      const s = getSelection(); s.removeAllRanges(); s.addRange(pos);
      return true;
    }
    void y;
  }
  setFlat(t, dir > 0 ? 0 : flatLen(t));
  return true;
}

function caretRect() {
  const s = getSelection();
  if (!s.rangeCount) return null;
  const r = s.getRangeAt(0).cloneRange();
  r.collapse(true);
  let rect = r.getClientRects()[0];
  if (!rect) {
    const m = el('span', '', '​');
    r.insertNode(m);
    rect = m.getBoundingClientRect();
    const p = m.parentNode; m.remove(); p.normalize();
  }
  return rect;
}

function onKeyDown(e) {
  const mod = e.ctrlKey || e.metaKey;
  if (mod && e.key.toLowerCase() === 's') { e.preventDefault(); if (e.shiftKey) saveAs(); else save(); return; }
  if (mod && !e.shiftKey && e.key.toLowerCase() === 'z') { e.preventDefault(); undo('undo'); return; }
  if (mod && (e.key.toLowerCase() === 'y' || (e.shiftKey && e.key.toLowerCase() === 'z'))) { e.preventDefault(); undo('redo'); return; }
  if (!$('#modal').hidden) return;
  const blk = e.target.closest && e.target.closest('#page .blk');
  if (!blk) return;
  if (mod && e.key.toLowerCase() === 'a') {
    // 1º Ctrl+A: o parágrafo; 2º: o documento inteiro (para copiar)
    e.preventDefault();
    const sel = getSelection();
    const host = e.target.closest('[contenteditable="true"]') || blk;
    const whole = sel.rangeCount && sel.toString().length >= host.textContent.length - 1 && host.textContent.length > 0;
    if (whole) selectWholeDoc();
    else { const r = document.createRange(); r.selectNodeContents(host); sel.removeAllRanges(); sel.addRange(r); }
    return;
  }
  // seleção que passa por vários blocos: só copiar (digitar ou apagar estragaria o texto)
  if (spansBlocks() && (e.key.length === 1 || e.key === 'Backspace' || e.key === 'Delete' || e.key === 'Enter') && !mod) {
    e.preventDefault();
    toast('A seleção pega vários blocos: dá para copiar (Ctrl+C). Para apagar blocos, use Excluir na barra.');
    return;
  }
  if (S.busy) { if (!['Shift', 'Control', 'Alt', 'Meta'].includes(e.key) && e.key.length !== 1) e.preventDefault(); return; }
  if (mod && ['b', 'i', 'u'].includes(e.key.toLowerCase())) { e.preventDefault(); format({ b: 'bold', i: 'italic', u: 'underline' }[e.key.toLowerCase()]); return; }
  if (mod && (e.key === '=' || e.key === '+')) { e.preventDefault(); format(e.shiftKey ? 'superscript' : 'subscript'); return; }
  if (mod && e.key.toLowerCase() === 'm') { e.preventDefault(); insertMath(); return; }
  const sel = getSelection();
  const collapsed = sel.rangeCount && sel.getRangeAt(0).collapsed;
  if (e.key === 'Enter') {
    e.preventDefault();
    if (blk.dataset.phantom) {
      // linha vazia não existe no LaTeX: Enter nela vira espaço vertical de verdade
      if (!blk.textContent.trim()) phantomToSpace(blk);
      return;
    }
    if (e.shiftKey) { insertAtom('\\\\\n', 'br'); return; }
    if (blk.closest('.macro')) return;
    const off = getFlat(blk), len = flatLen(blk);
    if (blk.classList.contains('heading') || off >= len) { flushThen(() => makePhantom(blk, 'after')); return; }
    if (off === 0) { flushThen(() => makePhantom(blk, 'before')); return; }
    const r = hoistedRange(blk);
    const pm = el('span', 'parmark');
    r.insertNode(pm);
    const after = document.createRange();
    after.setStartAfter(pm); after.collapse(true);
    sel.removeAllRanges(); sel.addRange(after);
    S.dirtyId = blk.dataset.id;
    clearTimeout(S.timer);
    const inl = readInl(blk, []).map(n => n);
    insertCaretAfterPar(inl);
    S.dirtyId = null;
    withBusy(async () => {
      const res = await api('edit', { path: S.path, id: +blk.dataset.id, inl, structural: true });
      applyState(res); renderDoc(res.focus);
    });
    return;
  }
  if (e.key === 'Backspace' && collapsed && getFlat(blk) === 0 && !e.target.closest('.marg')) {
    e.preventDefault();
    if (blk.dataset.phantom) { const prev = navBlocks()[navBlocks().indexOf(blk) - 1]; blk.remove(); if (prev) setFlat(prev, flatLen(prev)); return; }
    commitDirty(false);
    op('merge', { id: +blk.dataset.id });
    return;
  }
  if (e.key === 'Delete' && collapsed && getFlat(blk) >= flatLen(blk) && !e.target.closest('.marg')) {
    const next = navBlocks()[navBlocks().indexOf(blk) + 1];
    if (next && !next.dataset.phantom) {
      e.preventDefault();
      commitDirty(false);
      op('merge', { id: +next.dataset.id });
    }
    return;
  }
  if (!collapsed || e.shiftKey || mod) return;
  if (e.key === 'ArrowLeft' && getFlat(blk) === 0) { if (moveTo(blk, -1)) e.preventDefault(); return; }
  if (e.key === 'ArrowRight' && getFlat(blk) >= flatLen(blk)) { if (moveTo(blk, 1)) e.preventDefault(); return; }
  if (e.key === 'ArrowUp' || e.key === 'ArrowDown') {
    const rc = caretRect(); const br = blk.getBoundingClientRect();
    if (!rc) return;
    const lh = parseFloat(getComputedStyle(blk).lineHeight) || 24;
    if (e.key === 'ArrowUp' && rc.top - br.top < lh * 0.8) { if (moveTo(blk, -1, rc.left)) e.preventDefault(); }
    if (e.key === 'ArrowDown' && br.bottom - rc.bottom < lh * 0.8) { if (moveTo(blk, 1, rc.left)) e.preventDefault(); }
  }
}

function selectWholeDoc() {
  const page = $('#page');
  const r = document.createRange();
  r.selectNodeContents(page);
  const sel = getSelection();
  sel.removeAllRanges();
  sel.addRange(r);
}
function spansBlocks() {
  const sel = getSelection();
  if (!sel.rangeCount || sel.isCollapsed) return false;
  const r = sel.getRangeAt(0);
  const at = n => (n.nodeType === 3 ? n.parentElement : n).closest('#page .blk');
  const a = at(r.startContainer), b = at(r.endContainer);
  return !a || a !== b;
}

function flushThen(fn) {
  commitDirty(false);
  // fn pode enfileirar outra operação: não espera por ela (senão a fila trava)
  enqueue(async () => { fn(); });
}

function onBeforeInput(e) {
  if (!e.target.closest || !e.target.closest('#page')) return;
  if (spansBlocks()) { e.preventDefault(); return; }
  if (S.busy) {
    e.preventDefault();
    if (e.inputType === 'insertText' && e.data) S.buffer += e.data;
    return;
  }
  if (e.inputType === 'insertParagraph' || e.inputType === 'insertLineBreak') { e.preventDefault(); return; }
  if (e.inputType === 'insertText' && e.data === '"') {
    e.preventDefault();
    const s = getSelection(); const r = s.getRangeAt(0);
    let prev = '';
    if (r.startContainer.nodeType === 3 && r.startOffset > 0) prev = r.startContainer.data[r.startOffset - 1];
    document.execCommand('insertText', false, !prev || /[\s(\[ —–]/.test(prev) ? '“' : '”');
  }
}

function onInput(e) {
  const blk = e.target.closest && e.target.closest('#page .blk');
  if (!blk) return;
  if (blk.dataset.phantom) {
    clearTimeout(S.timer);
    S.dirtyId = blk.dataset.id;
    S.timer = setTimeout(() => commitDirty(false), 500);
    return;
  }
  markDirty(blk);
}

function onPaste(e) {
  const blk = e.target.closest && e.target.closest('#page .blk');
  if (!blk) return;
  e.preventDefault();
  if (spansBlocks()) { toast('A seleção pega vários blocos: cole dentro de um parágrafo.'); return; }
  const text = (e.clipboardData.getData('text/plain') || '').replace(/\r\n/g, '\n');
  const parts = text.split(/\n\s*\n/).map(p => p.replace(/\s*\n\s*/g, ' '));
  if (parts.length === 1 || blk.dataset.phantom || blk.closest('.macro') || blk.classList.contains('heading')) {
    document.execCommand('insertText', false, parts.join(' '));
    return;
  }
  const sel = getSelection();
  const r = sel.getRangeAt(0);
  r.deleteContents();
  const frag = document.createDocumentFragment();
  parts.forEach((p, k) => {
    if (k) frag.append(el('span', 'parmark'));
    frag.append(document.createTextNode(p));
  });
  const caret = el('span', 'caretmark');
  frag.append(caret);
  r.insertNode(frag);
  S.dirtyId = blk.dataset.id;
  clearTimeout(S.timer);
  const inl = readInl(blk, []);
  withBusy(async () => {
    const res = await api('edit', { path: S.path, id: +blk.dataset.id, inl, structural: true });
    applyState(res); renderDoc(res.focus);
  });
  S.dirtyId = null;
}

function format(cmd) {
  const blk = curBlock();
  if (!blk || blk.classList.contains('locked')) return;
  document.execCommand(cmd);
  if (blk.dataset.phantom) return;
  structuralEdit(blk);
}

// --------------------------------------------------------- atomos/modal
function modal({ title, src, preview, del, onOk, onDel, readOnlyHtml, form, okText }) {
  const m = $('#modal'), ta = $('#m-src'), pv = $('#m-prev');
  $('#m-title').textContent = title;
  $('#m-del').hidden = !del;
  ta.hidden = !!(readOnlyHtml || form);
  $('#m-ok').hidden = !!readOnlyHtml;
  $('#m-ok').textContent = okText || 'Aplicar';
  pv.innerHTML = '';
  if (readOnlyHtml || form) { pv.append(readOnlyHtml || form); pv.style.maxHeight = '64vh'; } else pv.style.maxHeight = '';
  if (form) preview = null;
  ta.value = src || '';
  const paint = () => {
    if (readOnlyHtml || form) return;
    if (!preview) { pv.innerHTML = ''; return; }
    pv.innerHTML = '';
    const d = el('div');
    preview(ta.value, d);
    pv.append(d);
  };
  ta.oninput = paint;
  paint();
  m.hidden = false;
  if (form) { const f = form.querySelector("input[type=text]"); if (f) { f.focus(); f.select(); } } else if (!readOnlyHtml) { ta.focus(); }
  const close = () => { m.hidden = true; ta.oninput = null; document.removeEventListener('keydown', key, true); };
  const ok = () => { close(); onOk && onOk(ta.value); };
  const key = ev => {
    if (ev.key === 'Escape') { ev.preventDefault(); close(); }
    if (ev.key === 'Enter' && (ev.ctrlKey || ev.metaKey)) { ev.preventDefault(); if (!readOnlyHtml) ok(); }
  };
  document.addEventListener('keydown', key, true);
  $('#m-ok').onclick = ok;
  $('#m-cancel').onclick = close;
  $('#m-del').onclick = () => { close(); onDel && onDel(); };
}

function previewFor(kind) {
  if (kind === 'math' || kind === 'dmath') return (v, d) => katexInto(d, stripMath(v.trim()), kind === 'dmath');
  if (kind === 'ce') return (v, d) => katexInto(d, v, false);
  if (kind === 'si') return (v, d) => (d.textContent = siText(v));
  return null;
}

function editAtom(a) {
  const blk = a.closest('.blk');
  if (!blk) return;
  const kind = a.dataset.kind;
  const names = { math: 'Fórmula', dmath: 'Fórmula em destaque', ce: 'Fórmula química (\\ce)', si: 'Grandeza (siunitx)', ref: 'Referência', cmd: 'Comando LaTeX', env: 'Ambiente', layout: 'Comando de layout', comment: 'Comentário', img: 'Imagem', br: 'Quebra de linha', sym: 'Símbolo' };
  modal({
    title: names[kind] || 'Código',
    src: a.dataset.src,
    preview: previewFor(kind),
    del: true,
    onOk: v => {
      if (!v.trim()) { a.remove(); } else { a.dataset.src = v; paintAtom(a); }
      structuralEdit(blk);
    },
    onDel: () => { a.remove(); structuralEdit(blk); },
  });
}

function editMacroRaw(mraw) {
  const mac = mraw.closest('.macro'), blk = mac.closest('.blk');
  const meta = JSON.parse(mac.dataset.meta);
  const idx = [...mac.querySelectorAll(':scope > .mraw, :scope > .marg, :scope > .capwrap > .marg')].indexOf(mraw);
  const a = meta.args[idx];
  if (!a) return;
  modal({
    title: 'Argumento de ' + meta.name,
    src: a.raw,
    onOk: v => { a.raw = v.replace(/\n/g, ' '); mac.dataset.meta = JSON.stringify(meta); mraw.textContent = a.opt ? `[${a.raw}]` : a.raw; structuralEdit(blk); },
  });
}

function editRaw(id) {
  commitDirty(false);
  const b = S.blocks[id];
  if (!b) return;
  const kind = b.kind === 'math' ? 'dmath' : null;
  modal({
    title: b.env ? `Código: ${b.env}` : 'Código LaTeX',
    src: b.src.replace(/\n$/, ''),
    preview: kind ? (v, d) => { const t = v.trim(); katexInto(d, t.startsWith('\\[') || t.startsWith('$$') ? stripMath(t) : t, true); } : null,
    del: true,
    onOk: v => op('raw', { id, src: v }),
    onDel: () => op('raw', { id, src: '' }),
  });
}

let savedRange = null;
function insertAtom(src, kind) {
  const blk = curBlock();
  if (!blk || blk.dataset.phantom) return;
  const a = renderAtom({ src, kind });
  const r = getSelection().getRangeAt(0);
  r.deleteContents();
  r.insertNode(a);
  r.setStartAfter(a); r.collapse(true);
  structuralEdit(blk);
}
function insertMath() {
  const blk = curBlock();
  if (!blk || blk.dataset.phantom || blk.classList.contains('locked')) { toast('Coloque o cursor num parágrafo primeiro.'); return; }
  const sel = getSelection();
  savedRange = sel.rangeCount ? sel.getRangeAt(0).cloneRange() : null;
  const selected = savedRange ? savedRange.toString() : '';
  modal({
    title: 'Inserir fórmula',
    src: '$' + (selected || 'x^2') + '$',
    preview: previewFor('math'),
    onOk: v => {
      if (!v.trim()) return;
      const s = getSelection(); s.removeAllRanges(); if (savedRange) s.addRange(savedRange);
      const kind = v.trim().startsWith('$$') || v.trim().startsWith('\\[') ? 'dmath' : 'math';
      insertAtom(v.trim(), kind);
    },
  });
  const ta = $('#m-src'); ta.setSelectionRange(1, ta.value.length - 1);
}

function onClick(e) {
  const t = e.target;
  if (!t.closest) return;
  const atom = t.closest('#page .atom');
  if (atom && !atom.closest('.raw')) { e.preventDefault(); editAtom(atom); return; }
  const mraw = t.closest('#page .mraw');
  if (mraw) { e.preventDefault(); editMacroRaw(mraw); return; }
  // caixas, tabelas, comandos e espaços: um clique seleciona, dois editam o código
  const h = t.closest('#page .env-h, #page .raw, #page .cmd, #page .vsp');
  if (h && !t.closest('button')) { if (e.detail >= 2) editRaw(+h.dataset.id); return; }
  const raw = t.closest('#page .comment, #page .blk.locked');
  if (raw) { editRaw(+raw.dataset.id); return; }
}

function onFocusIn(e) {
  const blk = e.target.closest && e.target.closest('#page .blk');
  if (!blk || blk.dataset.phantom) return;
  if (S.lastId !== blk.dataset.id) {
    if (S.dirtyId != null && S.dirtyId !== blk.dataset.id) commitDirty(false);
    S.lastId = blk.dataset.id;
    syncStyleSelect();
    renderSource();
  }
}

function syncStyleSelect() {
  const b = S.blocks[S.lastId];
  const s = $('#style');
  s.disabled = !b || b.kind === 'item';
  if (!b) return;
  s.value = b.kind === 'heading' ? b.env : 'para';
}

// ------------------------------------------------------------- fonte
function renderSource() {
  if (!S.showSource || S.srcEditing) return;
  const v = $('#src-view');
  let before = '', cur = '', after = '';
  const id = S.lastId != null ? +S.lastId : -1;
  for (const b of S.blocks) {
    if (b.id < id) before += b.src; else if (b.id === id) cur = b.src; else after += b.src;
  }
  v.innerHTML = esc(before) + '<mark>' + esc(cur) + '</mark>' + esc(after);
  const m = $('mark', v);
  if (m && cur) {
    const vr = v.getBoundingClientRect(), mr = m.getBoundingClientRect();
    if (mr.top < vr.top || mr.bottom > vr.bottom) v.scrollTop += mr.top - vr.top - vr.height / 3;
  }
}

// ------------------------------------------------------------ arquivos
function renderTree() {
  const q = $('#filter').value.trim().toLowerCase();
  const tree = $('#tree');
  tree.textContent = '';
  let lastDir = null;
  for (const f of S.files) {
    if (q && !f.toLowerCase().includes(q)) continue;
    const i = f.lastIndexOf('/');
    const dir = i >= 0 ? f.slice(0, i) : '';
    if (dir !== lastDir) { tree.append(el('div', 'dir', dir || '/')); lastDir = dir; }
    const e = el('div', 'f', f.slice(i + 1));
    e.dataset.path = f;
    e.title = f;
    if (f === S.path) e.classList.add('cur');
    if (S.dirtyFiles.has(f)) e.classList.add('dirty');
    e.onclick = () => openFile(f);
    tree.append(e);
  }
}

let openSeq = 0;
async function openFile(path, reload) {
  const seq = ++openSeq;
  await commitDirty(false);
  await chain;
  try {
    const r = await api(reload ? 'reload' : 'open', { path });
    if (seq !== openSeq) return;
    showDoc(r);
    if (r.locked) toast(`${r.locked} trecho(s) não puderam ser interpretados e ficaram só como código (em vermelho claro).`);
  } catch (e) { toast(e.message, true); }
}

async function save(force) {
  if (!S.path) return;
  await commitDirty(false);
  await chain;
  try {
    // grava todos os arquivos alterados (uma mudança de layout pode mexer no preâmbulo também)
    const r = await api('saveall', { path: S.path, force: !!force });
    applyState(r);
    for (const f of r.saved) S.dirtyFiles.delete(f);
    renderTree();
    if (r.errors.length) toast(r.errors.join(' · '), true);
    else if (r.conflicts.length) {
      if (confirm(`Estes arquivos foram alterados fora do editor: ${r.conflicts.join(', ')}.\n\nSobrescrever mesmo assim? (A versão do disco vai para o backup.)`)) save(true);
    } else toast(r.saved.length ? 'Salvo: ' + r.saved.join(', ') : 'Nada para salvar.');
    $('#banner').hidden = true;
  } catch (e) { toast(e.message, true); }
}

async function refreshDirtyFiles() {
  try {
    const r = await api('files', {});
    S.dirtyFiles = new Set(r.dirty);
    renderTree();
  } catch (_) { /* ignora */ }
}

function showDoc(r) {
  S.path = r.path; S.styles = r.styles || {}; S.lastId = null;
  if (r.macros) S.macros = r.macros;
  hooks.open.forEach(f => f(r));
  applyState(r);
  $('#fname').textContent = r.path;
  document.title = r.path.split('/').pop() + ' — TeX Editor';
  $('#banner').hidden = true;
  renderTree();
  renderDoc(null);
  $('#doc').scrollTop = 0;
  try { localStorage.setItem('tex-editor:last', r.path); } catch (_) { /* sem storage */ }
}

async function saveAs() {
  if (!S.path) return;
  await commitDirty(false);
  await chain;
  const inputs = S.blocks.filter(b => b.kind === 'cmd' && (b.env === 'input' || b.env === 'include'));
  const isMain = S.blocks.some(b => b.kind === 'cmd' && b.env === 'documentclass');
  const stem = S.path.replace(/\.tex$/, '');
  const f = el('div', 'saveas');
  const opt = (value, title, desc) => {
    const l = el('label', 'sa-opt');
    const r = el('input'); r.type = 'radio'; r.name = 'sa-mode'; r.value = value;
    const t = el('div'); t.append(el('b', '', title), el('div', 'sa-desc', desc));
    l.append(r, t);
    return l;
  };
  f.append(opt('copy', 'Cópia deste arquivo',
    'Grava o arquivo como ele está agora, com as linhas \\input intactas: elas continuam apontando para os mesmos arquivos. O editor passa a trabalhar na cópia; o original fica como estava no disco.'));
  f.append(opt('flat', 'Arquivo .tex completo',
    'Troca cada \\input e \\include pelo conteúdo do arquivo, recursivamente, e grava tudo num único .tex. Os trechos que ainda não foram salvos entram como estão no editor. O arquivo atual continua aberto.'));
  const info = el('div', 'sa-info');
  const paint = () => {
    info.textContent = '';
    const mode = f.querySelector('input[name=sa-mode]:checked').value;
    if (mode === 'flat') {
      if (!inputs.length) info.append(el('div', 'warn', 'Este arquivo não tem \\input nem \\include: o resultado será igual a uma cópia.'));
      else info.append(el('div', '', `Entram ${inputs.length} arquivo(s) diretamente: ` + inputs.map(b => (b.target || b.src.trim())).join(', ') + ' (e o que eles incluírem).'));
      if (!isMain) info.append(el('div', 'warn', 'Este arquivo não tem \\documentclass. Para um .tex que compila sozinho, abra o arquivo principal (ex.: main.tex ou wrapper_capXX.tex) e use esta opção nele.'));
    } else if (inputs.length) {
      info.append(el('div', '', 'Os \\input usam caminhos relativos. Se você salvar em outra pasta, o LaTeX pode não achar os arquivos incluídos.'));
    }
    info.append(el('div', 'muted', 'Imagens (\\includegraphics, \\imagem) não são copiadas: o arquivo novo funciona na mesma pasta do original.'));
  };
  const name = el('input'); name.type = 'text'; name.spellcheck = false; name.className = 'sa-name';
  const setName = () => {
    const mode = f.querySelector('input[name=sa-mode]:checked').value;
    name.value = mode === 'flat' ? `${stem}-completo.tex` : `${stem}-copia.tex`;
  };
  f.querySelectorAll('input[name=sa-mode]').forEach(r => r.onchange = () => { setName(); paint(); });
  const first = f.querySelector(`input[value=${isMain && inputs.length ? 'flat' : 'copy'}]`);
  first.checked = true;
  const nl = el('label', 'sa-label', 'Caminho do arquivo, a partir da pasta aberta:');
  f.append(nl, name, info);
  setName(); paint();
  name.onkeydown = e => { if (e.key === 'Enter') { e.preventDefault(); $('#m-ok').click(); } };
  const run = async (to, mode, overwrite) => {
    try {
      const r = await api('saveas', { path: S.path, to, mode, overwrite });
      if (r.files) S.files = r.files;
      if (mode === 'flat') {
        renderTree();
        let msg = `Gerado ${r.written} com ${r.inlined.length} arquivo(s) incluído(s).`;
        if (r.missing.length) { msg += ` Não encontrei: ${r.missing.join(', ')}.`; toast(msg, true); } else toast(msg);
      } else {
        showDoc(r);
        toast('Salvo como ' + r.path);
      }
    } catch (e) {
      if (e.status === 409 && confirm(e.message + '\n\nSubstituir?')) run(to, mode, true);
      else if (e.status !== 409) toast(e.message, true);
    }
  };
  modal({
    title: 'Salvar como',
    form: f,
    okText: 'Salvar',
    onOk: () => run(name.value.trim(), f.querySelector('input[name=sa-mode]:checked').value, false),
  });
}

async function undo(which) {
  await commitDirty(false);
  op(which, {});
}

async function showDiff() {
  await commitDirty(false);
  await chain;
  const r = await api('diff', { path: S.path });
  const box = el('div', 'diff');
  if (!r.diff.trim()) box.append(el('div', 'gap', 'Nenhuma alteração desde o último salvamento.'));
  for (const line of r.diff.split('\n')) {
    if (!line) continue;
    const c = line === '@@' ? 'gap' : line[5] === '+' ? 'add' : line[5] === '-' ? 'del' : '';
    box.append(el('div', c, line === '@@' ? '⋯' : line));
  }
  modal({ title: 'Alterações (disco → editor)', readOnlyHtml: box });
}

let polling = false;
async function poll() {
  if (polling || !S.path || document.hidden) return;
  polling = true;
  try {
    const r = await api('check', { path: S.path });
    if (r.changed) {
      if (!r.dirty && S.dirtyId == null) { await openFile(S.path, true); toast('O arquivo mudou no disco e foi recarregado.'); }
      else {
        const b = $('#banner');
        b.innerHTML = '';
        b.append(el('span', '', 'Este arquivo foi alterado fora do editor.'));
        const re = el('button', 'tb wide', 'Recarregar do disco (descarta o que não foi salvo)');
        re.onclick = () => openFile(S.path, true);
        b.append(re);
        b.hidden = false;
      }
    }
  } catch (_) { /* servidor fora do ar */ } finally { polling = false; }
}

// ------------------------------------------------------------- início
async function init() {
  document.execCommand('styleWithCSS', false, false);
  const page = $('#page');
  page.addEventListener('keydown', onKeyDown);
  document.addEventListener('keydown', e => {
    if (e.target.closest && e.target.closest('#page')) return;
    const mod = e.ctrlKey || e.metaKey;
    if (!mod) return;
    const k = e.key.toLowerCase();
    if (k === 's') { e.preventDefault(); if (e.shiftKey) saveAs(); else save(); return; }
    if (k === 'a' && !/^(INPUT|TEXTAREA|SELECT)$/.test(e.target.tagName || '') && $('#modal').hidden) { e.preventDefault(); selectWholeDoc(); return; }
    const typing = /^(INPUT|TEXTAREA|SELECT)$/.test(e.target.tagName || '') || !$('#modal').hidden;
    if (typing) return;
    if (k === 'z' && !e.shiftKey) { e.preventDefault(); undo('undo'); }
    else if (k === 'y' || (k === 'z' && e.shiftKey)) { e.preventDefault(); undo('redo'); }
  });
  page.addEventListener('beforeinput', onBeforeInput);
  page.addEventListener('input', onInput);
  page.addEventListener('paste', onPaste);
  page.addEventListener('click', onClick);
  page.addEventListener('focusin', onFocusIn);
  page.addEventListener('focusout', e => {
    const blk = e.target.closest && e.target.closest('#page .blk');
    if (blk && S.dirtyId === blk.dataset.id && !blk.contains(e.relatedTarget)) commitDirty(false);
  });
  document.querySelectorAll('.tb.fmt').forEach(b => b.addEventListener('mousedown', ev => { ev.preventDefault(); format(b.dataset.cmd); }));
  $('#b-math').addEventListener('mousedown', ev => { ev.preventDefault(); insertMath(); });
  $('#b-undo').onclick = () => undo('undo');
  $('#b-redo').onclick = () => undo('redo');
  $('#b-save').onclick = () => save();
  $('#b-saveas').onclick = () => saveAs();
  $('#b-diff').onclick = showDiff;
  $('#b-files').onclick = () => $('#files').classList.toggle('hide');
  $('#b-codes').onclick = () => { document.body.classList.toggle('codes'); $('#b-codes').classList.toggle('on'); };
  $('#b-source').onclick = () => {
    S.showSource = !S.showSource;
    $('#source').hidden = !S.showSource;
    $('#b-source').classList.toggle('on', S.showSource);
    renderSource();
  };
  $('#b-src-edit').onclick = async () => {
    await commitDirty(false); await chain;
    S.srcEditing = true;
    const ta = $('#src-edit');
    ta.value = S.blocks.map(b => b.src).join('');
    $('#src-view').hidden = true; ta.hidden = false;
    $('#b-src-edit').hidden = true; $('#b-src-apply').hidden = false; $('#b-src-cancel').hidden = false;
    const mark = S.blocks[S.lastId];
    if (mark) { ta.focus(); ta.setSelectionRange(mark.start, mark.start); }
  };
  const endSrcEdit = () => {
    S.srcEditing = false;
    $('#src-view').hidden = false; $('#src-edit').hidden = true;
    $('#b-src-edit').hidden = false; $('#b-src-apply').hidden = true; $('#b-src-cancel').hidden = true;
    renderSource();
  };
  $('#b-src-apply').onclick = async () => {
    const src = $('#src-edit').value;
    endSrcEdit();
    op('setsource', { src });
  };
  $('#b-src-cancel').onclick = endSrcEdit;
  $('#style').onchange = async () => {
    const id = S.lastId;
    const to = $('#style').value;
    if (id == null) return;
    await commitDirty(false);
    op('retype', { id: +id, to });
  };
  $('#filter').oninput = renderTree;
  window.addEventListener('beforeunload', e => { if (S.dirtyFiles.size || S.dirtyId != null) { e.preventDefault(); e.returnValue = ''; } });
  setInterval(poll, 4000);
  window.addEventListener('focus', poll);

  // KaTeX vem da CDN; espera um pouco por ele antes do primeiro desenho
  for (let i = 0; i < 40 && !window.katex; i++) await new Promise(r => setTimeout(r, 50));

  const r = await api('init');
  S.files = r.files;
  let last = null;
  try { last = localStorage.getItem('tex-editor:last'); } catch (_) { /* sem storage */ }
  const first = r.initial || (last && S.files.includes(last) ? last : null) || S.files[0];
  renderTree();
  if (first) openFile(first);
  else $('#page').append(el('div', 'empty-doc', 'Nenhum arquivo .tex nesta pasta.'));
}

init().catch(e => toast(e.message, true));
