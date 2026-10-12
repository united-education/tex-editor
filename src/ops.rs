//! Operações de diagramação sobre o fonte: mover blocos, colunas
//! (minipages lado a lado), quebra de página, imagens.
//!
//! Formato canônico de uma linha de colunas:
//!   \par\noindent
//!   \begin{minipage}[t]{0.48\linewidth}
//!   ...
//!   \end{minipage}\hfill
//!   \begin{minipage}[t]{0.48\linewidth}
//!   ...
//!   \end{minipage}\par

use crate::latex::{parse, Block, Inline};

pub type Res = Result<(String, usize), String>;

pub const GAP: f64 = 0.04;

fn is_boundary_start(blocks: &[Block], s: usize) -> bool {
    blocks.iter().any(|b| b.start == s)
}
fn is_boundary_end(blocks: &[Block], e: usize) -> bool {
    blocks.iter().any(|b| b.end == e)
}

/// Bloco "cola": só espaços, comentários ou comandos de layout (\par, \noindent, \hfill...).
fn is_glue(b: &Block) -> bool {
    match b.kind {
        "blank" => true,
        "para" => b.inl.iter().all(|n| match n {
            Inline::Text { v, .. } => v.trim().is_empty(),
            Inline::Atom { kind, .. } => kind == "layout",
            _ => false,
        }),
        _ => false,
    }
}

/// Índice do `end` que fecha o `begin` em i.
pub fn matching_end(blocks: &[Block], i: usize) -> Option<usize> {
    let mut d = 0i32;
    for (k, b) in blocks.iter().enumerate().skip(i) {
        if b.kind == "begin" {
            d += 1;
        } else if b.kind == "end" {
            d -= 1;
            if d == 0 {
                return Some(k);
            }
        }
    }
    None
}

#[derive(Clone, Debug)]
pub struct Col {
    pub begin: usize,
    pub end: usize,
    pub width: f64,
    pub content: String,
}

#[derive(Clone, Debug)]
pub struct Row {
    pub start: usize,
    pub end: usize,
    pub cols: Vec<Col>,
    /// `\par\noindent\hfill` antes: a linha (de uma coluna) fica à direita
    pub right: bool,
}

pub fn width_of(begin_src: &str) -> f64 {
    // {0.48\linewidth}
    for unit in ["\\linewidth", "\\textwidth", "\\columnwidth", "\\hsize"] {
        if let Some(k) = begin_src.find(unit) {
            let before = &begin_src[..k];
            let num: String = before.chars().rev().take_while(|c| c.is_ascii_digit() || *c == '.').collect::<Vec<_>>().into_iter().rev().collect();
            return num.parse().unwrap_or(1.0);
        }
    }
    1.0
}

pub fn rows(src: &str, blocks: &[Block]) -> Vec<Row> {
    let mut out = vec![];
    let mut i = 0;
    while i < blocks.len() {
        if blocks[i].kind == "begin" && blocks[i].env == "minipage" {
            let depth = blocks[i].depth;
            let mut cols = vec![];
            let mut k = i;
            let mut last_end = i;
            loop {
                let Some(e) = matching_end(blocks, k) else { break };
                let b = &blocks[k];
                let mut inner = src[b.end..blocks[e].start].trim();
                if let Some(r) = inner.strip_prefix("\\vspace{0pt}") {
                    inner = r.trim_start();
                }
                let inner = inner.to_string();
                cols.push(Col { begin: k, end: e, width: width_of(&b.src), content: inner });
                last_end = e;
                // procura a próxima minipage depois de cola
                let mut j = e + 1;
                while j < blocks.len() && is_glue(&blocks[j]) && blocks[j].kind != "blank" {
                    j += 1;
                }
                if j < blocks.len() && blocks[j].kind == "begin" && blocks[j].env == "minipage" && blocks[j].depth == depth {
                    k = j;
                } else {
                    break;
                }
            }
            if cols.is_empty() {
                i += 1;
                continue;
            }
            // cola imediatamente antes (\par\noindent) e depois (\par)
            let mut start = blocks[i].start;
            let mut right = false;
            if i > 0 && blocks[i - 1].kind == "para" && is_glue(&blocks[i - 1]) {
                start = blocks[i - 1].start;
                right = blocks[i - 1].src.contains("\\hfill");
            }
            let mut end = blocks[last_end].end;
            if last_end + 1 < blocks.len() && blocks[last_end + 1].kind == "para" && is_glue(&blocks[last_end + 1]) && !src[blocks[last_end].start..blocks[last_end].end].ends_with('\n') {
                end = blocks[last_end + 1].end;
            }
            out.push(Row { start, end, cols, right });
            i = last_end + 1;
        } else {
            i += 1;
        }
    }
    out
}

fn fmt_w(w: f64) -> String {
    let s = format!("{:.2}", (w * 100.0).floor() / 100.0);
    s
}

pub fn canonical(cols: &[(f64, String)]) -> String {
    let mut s = String::from("\\par\\noindent\n");
    for (i, (w, c)) in cols.iter().enumerate() {
        // \vspace{0pt} alinha as colunas pelo topo (sem ele, [t] alinha pela
        // primeira linha, e uma imagem empurra a coluna vizinha para baixo)
        s.push_str(&format!("\\begin{{minipage}}[t]{{{}\\linewidth}}\n\\vspace{{0pt}}\n\n", fmt_w(*w)));
        let c = c.trim();
        if !c.is_empty() {
            s.push_str(c);
            s.push('\n');
        }
        s.push_str("\\end{minipage}");
        s.push_str(if i + 1 < cols.len() { "\\hfill\n" } else { "\\par\n" });
    }
    s
}

pub fn equal_widths(n: usize) -> Vec<f64> {
    let total = 1.0 - GAP * (n.saturating_sub(1)) as f64;
    vec![total / n as f64; n]
}

/// Dentro de caixas/colunas uma figura flutuante não funciona: usa a versão fixa.
pub fn unfloat(text: &str) -> String {
    text.replace("\\imagem{", "\\imagemH{").replace("\\imagem[", "\\imagemH[")
}

/// Ambiente mais interno aberto na posição pos.
pub fn env_at(blocks: &[Block], pos: usize) -> Vec<String> {
    let mut st: Vec<String> = vec![];
    for b in blocks {
        if b.start >= pos {
            break;
        }
        if b.kind == "begin" {
            st.push(b.env.clone());
        } else if b.kind == "end" {
            st.pop();
        }
    }
    st
}

fn in_box(blocks: &[Block], pos: usize) -> bool {
    env_at(blocks, pos).iter().any(|e| e != "document")
}

/// Junta linhas em branco em excesso ao redor de pos (no máximo uma).
pub fn tidy(src: &str, pos: usize) -> (String, usize) {
    let b = src.as_bytes();
    let pos = pos.min(src.len());
    let mut a = pos;
    while a > 0 && matches!(b[a - 1], b' ' | b'\t' | b'\n') {
        a -= 1;
    }
    let mut z = pos;
    while z < b.len() && matches!(b[z], b' ' | b'\t' | b'\n') {
        z += 1;
    }
    let region = &src[a..z];
    let nl = region.matches('\n').count();
    let indent: String = region.rsplit('\n').next().unwrap_or("").to_string();
    let rep = if z == src.len() && a > 0 {
        "\n".to_string()
    } else if a == 0 {
        indent.clone()
    } else if nl > 2 {
        format!("\n\n{}", indent)
    } else {
        return (src.to_string(), pos);
    };
    if rep == region {
        return (src.to_string(), pos);
    }
    let new = format!("{}{}{}", &src[..a], rep, &src[z..]);
    let np = if pos >= z { pos - (z - a) + rep.len() } else if pos > a { a + rep.len().min(pos - a) } else { pos };
    (new, np)
}

fn insert_unit(src: &str, pos: usize, text: &str) -> (String, usize) {
    let before = &src[..pos];
    let prefix = if before.is_empty() || before.ends_with("\n\n") {
        ""
    } else if before.ends_with('\n') {
        "\n"
    } else {
        "\n\n"
    };
    let body = format!("{}\n", text.trim_end());
    let after = &src[pos..];
    let suffix = if after.starts_with('\n') || after.is_empty() { "" } else { "\n" };
    let new = format!("{}{}{}{}{}", before, prefix, body, suffix, after);
    (new, pos + prefix.len())
}

fn check_range(blocks: &[Block], s: usize, e: usize) -> Result<(), String> {
    if s >= e || !is_boundary_start(blocks, s) || !is_boundary_end(blocks, e) {
        return Err("trecho inválido (o documento mudou?)".into());
    }
    Ok(())
}

/// Remove colunas vazias e desfaz linhas com uma coluna só.
pub fn cleanup_rows(src: String) -> String {
    let mut src = src;
    for _ in 0..8 {
        let blocks = parse(&src);
        let rs = rows(&src, &blocks);
        // só mexe em linhas que ficaram com coluna vazia (minipages avulsas do autor ficam como estão)
        let Some(r) = rs.iter().find(|r| r.cols.iter().any(|c| c.content.is_empty())) else { break };
        let keep: Vec<&Col> = r.cols.iter().filter(|c| !c.content.is_empty()).collect();
        let rep = match keep.len() {
            0 => String::new(),
            1 => format!("{}\n", keep[0].content),
            _ => {
                let sum: f64 = keep.iter().map(|c| c.width).sum();
                let total = 1.0 - GAP * (keep.len() - 1) as f64;
                canonical(&keep.iter().map(|c| (c.width / sum * total, c.content.clone())).collect::<Vec<_>>())
            }
        };
        src = format!("{}{}{}", &src[..r.start], rep, &src[r.end..]);
    }
    src
}

pub fn move_unit(src: &str, s: usize, e: usize, to: usize) -> Res {
    let blocks = parse(src);
    check_range(&blocks, s, e)?;
    if to >= s && to <= e {
        return Ok((src.to_string(), s));
    }
    if !is_boundary_start(&blocks, to) && to != src.len() {
        return Err("destino inválido".into());
    }
    let text = src[s..e].to_string();
    let removed = format!("{}{}", &src[..s], &src[e..]);
    let to2 = if to > e { to - (e - s) } else { to };
    let b2 = parse(&removed);
    let text = if in_box(&b2, to2) { unfloat(&text) } else { text };
    let (ins, caret) = insert_unit(&removed, to2, &text);
    // arruma a emenda onde o bloco saiu
    let cut = if to2 <= s { s + (ins.len() - removed.len()) } else { s };
    let (t1, _) = tidy(&ins, cut);
    let delta = t1.len() as isize - ins.len() as isize;
    let caret = if cut < caret { (caret as isize + delta).max(0) as usize } else { caret };
    let (t2, caret) = tidy(&t1, caret);
    let out = cleanup_rows(t2);
    Ok((out.clone(), caret.min(out.len())))
}

/// Põe o bloco [s,e) ao lado do alvo [ts,te). Se o alvo é uma coluna,
/// cria uma coluna nova na mesma linha.
pub fn side_by_side(src: &str, s: usize, e: usize, ts: usize, te: usize, right: bool) -> Res {
    let blocks = parse(src);
    check_range(&blocks, s, e)?;
    check_range(&blocks, ts, te)?;
    if !(e <= ts || s >= te) {
        return Err("um bloco não pode ir ao lado dele mesmo".into());
    }
    let text = unfloat(src[s..e].trim());
    let removed = format!("{}{}", &src[..s], &src[e..]);
    let shift = |p: usize| if p >= e { p - (e - s) } else { p };
    let (ts2, te2) = (shift(ts), shift(te));
    let b2 = parse(&removed);
    // o alvo é uma coluna de uma linha existente?
    for r in rows(&removed, &b2) {
        if let Some(ci) = r.cols.iter().position(|c| b2[c.begin].start == ts2 && b2[c.end].end <= te2 + 1 && b2[c.end].end >= te2.saturating_sub(1)) {
            let mut cols: Vec<(f64, String)> = r.cols.iter().map(|c| (c.width, c.content.clone())).collect();
            let at = if right { ci + 1 } else { ci };
            cols.insert(at, (0.0, text.clone()));
            let ws = equal_widths(cols.len());
            for (k, c) in cols.iter_mut().enumerate() {
                c.0 = ws[k];
            }
            let rep = canonical(&cols);
            let out = format!("{}{}{}", &removed[..r.start], rep, &removed[r.end..]);
            let caret = r.start;
            return Ok((cleanup_rows(out), caret));
        }
    }
    let target = unfloat(removed[ts2..te2].trim());
    let ws = equal_widths(2);
    let cols = if right { vec![(ws[0], target), (ws[1], text)] } else { vec![(ws[0], text), (ws[1], target)] };
    let mut rep = canonical(&cols);
    if !removed[te2..].starts_with('\n') && !removed[..te2].ends_with('\n') {
        rep.push('\n');
    }
    let out = format!("{}{}{}", &removed[..ts2], rep, &removed[te2..]);
    let (out, caret) = tidy(&out, ts2);
    Ok((cleanup_rows(out), caret))
}

/// Linha que contém pos.
fn row_at(src: &str, pos: usize) -> Option<Row> {
    let blocks = parse(src);
    rows(src, &blocks).into_iter().find(|r| r.start <= pos && pos < r.end)
}

pub fn set_widths(src: &str, pos: usize, widths: &[f64]) -> Res {
    let r = row_at(src, pos).ok_or("colunas não encontradas")?;
    if widths.len() != r.cols.len() {
        return Err("número de colunas mudou".into());
    }
    let cols: Vec<(f64, String)> = r.cols.iter().zip(widths).map(|(c, w)| (w.clamp(0.05, 1.0), c.content.clone())).collect();
    let rep = canonical_row(&cols, r.right);
    Ok((format!("{}{}{}", &src[..r.start], rep, &src[r.end..]), r.start))
}

/// Como `canonical`; uma linha de uma coluna pode ficar à direita (\hfill antes).
pub fn canonical_row(cols: &[(f64, String)], right: bool) -> String {
    let s = canonical(cols);
    if right && cols.len() == 1 {
        s.replacen("\\par\\noindent\n", "\\par\\noindent\\hfill\n", 1)
    } else {
        s
    }
}

/// Troca a largura opcional da primeira macro de imagem do trecho: \imagemH[0.5]{ -> \imagemH[w]{.
fn set_img_opt(text: &str, name: &str, w: f64) -> String {
    let v = { let s = format!("{:.2}", w); s.trim_end_matches('0').trim_end_matches('.').to_string() };
    let with = format!("{}[", name);
    if let Some(k) = text.find(&with) {
        if let Some(e) = text[k + with.len()..].find(']') {
            return format!("{}{}{}{}", &text[..k + with.len()], v, "", &text[k + with.len() + e..]);
        }
    }
    let bare = format!("{}{{", name);
    if let Some(k) = text.find(&bare) {
        return format!("{}{}[{}]{}", &text[..k], name, v, &text[k + name.len()..]);
    }
    text.to_string()
}

fn img_opt(text: &str, name: &str) -> Option<f64> {
    let with = format!("{}[", name);
    let k = text.find(&with)?;
    let e = text[k + with.len()..].find(']')?;
    text[k + with.len()..k + with.len() + e].trim().parse().ok()
}

/// Imagem à esquerda, ao centro ou à direita. Esquerda/direita: a imagem vai
/// para uma minipage da largura dela (`\hfill` antes, se à direita), e ocupa
/// a minipage inteira. Centro: volta a ser a imagem sozinha (centralizada).
/// `fixed` é a macro de imagem fixa (\imagemH), `default` a largura padrão.
pub fn img_align(src: &str, s: usize, e: usize, side: &str, name: &str, fixed: &str, default: f64) -> Res {
    let blocks = parse(src);
    check_range(&blocks, s, e)?;
    let row = rows(src, &blocks).into_iter().find(|r| r.start == s && r.cols.len() == 1);
    let (w, content) = match &row {
        Some(r) => (r.cols[0].width, r.cols[0].content.clone()),
        None => {
            let t = src[s..e].trim().to_string();
            let w = img_opt(&t, name).unwrap_or(default);
            (w, t)
        }
    };
    let rep = if side == "center" {
        format!("{}\n", set_img_opt(&content, fixed, w))
    } else {
        let inner = set_img_opt(&unfloat(&content), fixed, 1.0);
        canonical_row(&[(w, inner)], side == "right")
    };
    let mut rep = rep;
    if !src[e..].starts_with('\n') && !rep.ends_with('\n') {
        rep.push('\n');
    }
    Ok((format!("{}{}{}", &src[..s], rep, &src[e..]), s))
}

pub fn unrow(src: &str, pos: usize) -> Res {
    let r = row_at(src, pos).ok_or("colunas não encontradas")?;
    let body: Vec<String> = r.cols.iter().filter(|c| !c.content.is_empty()).map(|c| c.content.clone()).collect();
    let rep = format!("{}\n", body.join("\n\n"));
    let out = format!("{}{}{}", &src[..r.start], rep, &src[r.end..]);
    let (out, c) = tidy(&out, r.start + rep.len());
    let _ = c;
    Ok((out, r.start))
}

pub fn add_column(src: &str, pos: usize) -> Res {
    let r = row_at(src, pos).ok_or("colunas não encontradas")?;
    let mut cols: Vec<(f64, String)> = r.cols.iter().map(|c| (0.0, c.content.clone())).collect();
    cols.push((0.0, "Texto da nova coluna.".into()));
    let ws = equal_widths(cols.len());
    for (k, c) in cols.iter_mut().enumerate() {
        c.0 = ws[k];
    }
    let rep = canonical(&cols);
    let out = format!("{}{}{}", &src[..r.start], rep, &src[r.end..]);
    let caret = r.start + rep.rfind("Texto da nova coluna.").unwrap_or(0);
    Ok((out, caret))
}

pub fn page_break(src: &str, pos: usize, cmd: &str) -> Res {
    let blocks = parse(src);
    if !is_boundary_start(&blocks, pos) {
        return Err("posição inválida".into());
    }
    let line = format!("\\{}\n", cmd);
    let prefix = if pos == 0 || src[..pos].ends_with('\n') { "" } else { "\n" };
    Ok((format!("{}{}{}{}", &src[..pos], prefix, line, &src[pos..]), pos + prefix.len() + line.len()))
}

pub fn delete_unit(src: &str, s: usize, e: usize) -> Res {
    let blocks = parse(src);
    check_range(&blocks, s, e)?;
    let out = format!("{}{}", &src[..s], &src[e..]);
    let (out, c) = tidy(&out, s);
    Ok((cleanup_rows(out), c))
}

pub fn duplicate_unit(src: &str, s: usize, e: usize) -> Res {
    let blocks = parse(src);
    check_range(&blocks, s, e)?;
    let text = src[s..e].to_string();
    let (out, c) = insert_unit(src, e, &text);
    Ok((out, c))
}

/// Faz a definição \newcommand{\nome}[N]{...} aceitar a largura como
/// argumento opcional: [N+1][padrão]. Devolve o novo fonte do arquivo.
pub fn add_width_option(src: &str, name: &str) -> Result<String, String> {
    let head = format!("\\newcommand{{\\{}}}[", name);
    let a = src.find(&head).ok_or_else(|| format!("definição de \\{} não encontrada", name))?;
    let after = &src[a + head.len()..];
    let close = after.find(']').ok_or("definição estranha")?;
    let n: u32 = after[..close].trim().parse().map_err(|_| "definição estranha".to_string())?;
    let rest = &after[close + 1..];
    if rest.starts_with('[') {
        return Ok(src.to_string()); // já tem opcional
    }
    let body_start = a + head.len() + close + 1;
    let p = crate::latex::P::new(src);
    if !src[body_start..].starts_with('{') {
        return Err("definição estranha".into());
    }
    let body_end = p.match_brace(body_start, src.len()).ok_or("definição sem fim")?;
    let body = &src[body_start..body_end];
    // largura atual: primeiro "X\linewidth" do corpo
    let k = body.find("\\linewidth").ok_or("a macro não usa \\linewidth")?;
    let num: String = body[..k].chars().rev().take_while(|c| c.is_ascii_digit() || *c == '.').collect::<Vec<_>>().into_iter().rev().collect();
    let default = if num.is_empty() { "1".to_string() } else { num.clone() };
    let mut nb = body.to_string();
    for i in (1..=n).rev() {
        nb = nb.replace(&format!("#{}", i), &format!("#{}", i + 1));
    }
    let pat = format!("{}\\linewidth", num);
    nb = nb.replace(&pat, "#1\\linewidth");
    Ok(format!("{}\\newcommand{{\\{}}}[{}][{}]{}{}", &src[..a], name, n + 1, default, nb, &src[body_end..]))
}

pub const SIZES: &[&str] = &["tiny", "scriptsize", "footnotesize", "small", "normalsize", "large", "Large", "LARGE"];
pub const ALIGNS: &[&str] = &["raggedright", "centering", "raggedleft"];

/// Tira as camadas \begin{small}...\end{small} / \begin{centering}... que
/// envolvem o trecho inteiro. Devolve (camadas de fora para dentro, miolo).
fn layers(text: &str) -> (Vec<String>, String) {
    let mut envs = vec![];
    let mut t = text.trim().to_string();
    loop {
        let bs = parse(&t);
        let idx: Vec<usize> = (0..bs.len()).filter(|&k| bs[k].kind != "blank").collect();
        let (Some(&f), Some(&l)) = (idx.first(), idx.last()) else { break };
        let b = &bs[f];
        if b.kind != "begin" || !(SIZES.contains(&b.env.as_str()) || ALIGNS.contains(&b.env.as_str())) || matching_end(&bs, f) != Some(l) {
            break;
        }
        envs.push(b.env.clone());
        t = t[b.end..bs[l].start].trim().to_string();
    }
    (envs, t)
}

fn wrap_layers(envs: &[String], inner: &str) -> String {
    let mut s = inner.trim().to_string();
    for e in envs.iter().rev() {
        s = format!("\\begin{{{e}}}\n{s}\n\n\\end{{{e}}}");
    }
    s
}

/// Tamanho da letra ("size") ou alinhamento ("align") de um bloco inteiro.
/// "normalsize" / "justify" tiram a camada.
pub fn set_decl(src: &str, s: usize, e: usize, kind: &str, value: &str) -> Res {
    let blocks = parse(src);
    check_range(&blocks, s, e)?;
    let set: &[&str] = if kind == "size" { SIZES } else { ALIGNS };
    if !set.contains(&value) && value != "justify" {
        return Err("valor inválido".into());
    }
    // o bloco já está dentro de \begin{small}/\begin{centering} só dele: usa o
    // envoltório inteiro (senão as camadas se acumulam uma dentro da outra)
    let (mut s, mut e) = (s, e);
    loop {
        let prev = blocks.iter().rposition(|b| b.end <= s && b.kind != "blank");
        let next = blocks.iter().position(|b| b.start >= e && b.kind != "blank");
        let (Some(p), Some(n)) = (prev, next) else { break };
        let (bp, bn) = (&blocks[p], &blocks[n]);
        let decl = |x: &str| SIZES.contains(&x) || ALIGNS.contains(&x);
        if bp.kind == "begin" && decl(&bp.env) && bn.kind == "end" && bn.env == bp.env && matching_end(&blocks, p) == Some(n) {
            s = bp.start;
            e = bn.end;
        } else {
            break;
        }
    }
    let (mut envs, inner) = layers(&src[s..e]);
    envs.retain(|x| !set.contains(&x.as_str()));
    let keep = !(value == "normalsize" || value == "justify");
    if keep {
        // alinhamento por fora, tamanho por dentro
        if kind == "align" {
            envs.insert(0, value.to_string());
        } else {
            envs.push(value.to_string());
        }
    }
    let mut text = wrap_layers(&envs, &inner);
    if src[s..e].ends_with('\n') {
        text.push('\n');
    }
    let out = format!("{}{}{}", &src[..s], text, &src[e..]);
    Ok((out, s))
}

/// Altura de um \vspace{...} em linhas (\baselineskip); None se não for um.
pub fn vspace_lines(t: &str) -> Option<f64> {
    let t = t.trim();
    let r = t.strip_prefix("\\vspace*{").or_else(|| t.strip_prefix("\\vspace{"))?.strip_suffix('}')?;
    let r = r.trim();
    let num = r.strip_suffix("\\baselineskip")?.trim();
    if num.is_empty() {
        return Some(1.0);
    }
    num.parse().ok()
}

fn fmt_lines(l: f64) -> String {
    let v = (l * 100.0).round() / 100.0;
    let s = format!("{}", v);
    if s == "1" { String::new() } else { s }
}

/// Espaço vertical antes ou depois do bloco [s,e), em linhas. 0 tira.
pub fn set_vspace(src: &str, s: usize, e: usize, side: &str, lines: f64) -> Res {
    let blocks = parse(src);
    check_range(&blocks, s, e)?;
    let neighbor = if side == "before" {
        blocks.iter().rev().filter(|b| b.end <= s).find(|b| b.kind != "blank")
    } else {
        blocks.iter().filter(|b| b.start >= e).find(|b| b.kind != "blank")
    };
    let text = format!("\\vspace{{{}\\baselineskip}}", fmt_lines(lines));
    let zero = lines.abs() < 0.01;
    if let Some(n) = neighbor.filter(|n| n.kind == "para" && vspace_lines(&n.src).is_some()) {
        let (a, z) = (n.start, n.end);
        let rep = if zero { String::new() } else { format!("{}\n", text) };
        let out = format!("{}{}{}", &src[..a], rep, &src[z..]);
        let (out, c) = tidy(&out, a);
        // o cursor vai para o espaço (ou, se ele saiu, para o bloco)
        let caret = if !zero { a } else if side == "before" { c } else { s };
        return Ok((out.clone(), caret.min(out.len())));
    }
    if zero {
        return Ok((src.to_string(), s));
    }
    let at = if side == "before" { s } else { e };
    let (out, c) = insert_unit(src, at, &text);
    Ok((out, c))
}

/// Largura do bloco: vira uma "linha" de uma coluna só (minipage). 1 desfaz.
pub fn block_width(src: &str, s: usize, e: usize, w: f64) -> Res {
    let blocks = parse(src);
    check_range(&blocks, s, e)?;
    let w = w.clamp(0.1, 1.0);
    if let Some(r) = rows(src, &blocks).into_iter().find(|r| r.start == s && r.cols.len() == 1) {
        if w > 0.995 {
            return unrow(src, r.start);
        }
        return set_widths(src, r.start, &[w]);
    }
    if w > 0.995 {
        return Ok((src.to_string(), s));
    }
    let text = unfloat(src[s..e].trim());
    let rep = canonical(&[(w, text)]);
    let out = format!("{}{}{}", &src[..s], rep, &src[e..]);
    Ok((out, s))
}

/// Inverte a ordem das colunas de uma linha (imagem à esquerda <-> à direita).
pub fn swap_cols(src: &str, pos: usize) -> Res {
    let r = row_at(src, pos).ok_or("colunas não encontradas")?;
    let cols: Vec<(f64, String)> = r.cols.iter().rev().map(|c| (c.width, c.content.clone())).collect();
    let rep = canonical(&cols);
    Ok((format!("{}{}{}", &src[..r.start], rep, &src[r.end..]), r.start))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn move_and_columns() {
        let s = "A um.\n\nB dois.\n\nC tres.\n";
        let b = parse(s);
        let paras: Vec<&Block> = b.iter().filter(|x| x.kind == "para").collect();
        let (out, _) = move_unit(s, paras[2].start, paras[2].end, paras[0].start).unwrap();
        assert_eq!(out, "C tres.\n\nA um.\n\nB dois.\n");
        let b = parse(&out);
        let p: Vec<&Block> = b.iter().filter(|x| x.kind == "para").collect();
        let (row, _) = side_by_side(&out, p[1].start, p[1].end, p[0].start, p[0].end, true).unwrap();
        assert!(row.contains("\\begin{minipage}[t]{0.48\\linewidth}\n\\vspace{0pt}\n\nC tres.\n\\end{minipage}\\hfill\n\\begin{minipage}[t]{0.48\\linewidth}\n\\vspace{0pt}\n\nA um.\n\\end{minipage}\\par\n"), "{}", row);
        // a linha volta a ser lida com o mesmo conteúdo
        let rb0 = parse(&row);
        let r0 = &rows(&row, &rb0)[0];
        assert_eq!(r0.cols[0].content, "C tres.");
        assert_eq!(r0.cols[1].content, "A um.");
        let rb = parse(&row);
        let rs = rows(&row, &rb);
        assert_eq!(rs.len(), 1);
        assert_eq!(rs[0].cols.len(), 2);
        let (un, _) = unrow(&row, rs[0].start).unwrap();
        assert_eq!(un, "C tres.\n\nA um.\n\nB dois.\n");
    }

    #[test]
    fn decl_space_width() {
        let s = "A um.\n\nB dois.\n";
        let b = parse(s);
        let p: Vec<&Block> = b.iter().filter(|x| x.kind == "para").collect();
        let (o, _) = set_decl(s, p[0].start, p[0].end, "size", "small").unwrap();
        assert_eq!(o, "\\begin{small}\nA um.\n\n\\end{small}\n\nB dois.\n");
        let ob = parse(&o);
        let (o2, _) = set_decl(&o, ob[0].start, ob.iter().find(|x| x.kind == "end").unwrap().end, "align", "centering").unwrap();
        assert_eq!(o2, "\\begin{centering}\n\\begin{small}\nA um.\n\n\\end{small}\n\n\\end{centering}\n\nB dois.\n");
        // de novo, mas pelo parágrafo de dentro: troca, não aninha
        let ob = parse(&o2);
        let inner = ob.iter().find(|x| x.kind == "para").unwrap();
        let (o2b, _) = set_decl(&o2, inner.start, inner.end, "align", "raggedleft").unwrap();
        assert_eq!(o2b, "\\begin{raggedleft}\n\\begin{small}\nA um.\n\n\\end{small}\n\n\\end{raggedleft}\n\nB dois.\n");
        let ob = parse(&o2);
        let last_end = ob.iter().rposition(|x| x.kind == "end").unwrap();
        let (o3, _) = set_decl(&o2, 0, ob[last_end].end, "size", "normalsize").unwrap();
        assert_eq!(o3, "\\begin{centering}\nA um.\n\n\\end{centering}\n\nB dois.\n");
        // espaço depois do primeiro parágrafo
        let (v, _) = set_vspace(s, p[0].start, p[0].end, "after", 1.0).unwrap();
        assert_eq!(v, "A um.\n\n\\vspace{\\baselineskip}\n\nB dois.\n");
        let vb = parse(&v);
        let q: Vec<&Block> = vb.iter().filter(|x| x.kind == "para").collect();
        let (v2, _) = set_vspace(&v, q[0].start, q[0].end, "after", 2.5).unwrap();
        assert_eq!(v2, "A um.\n\n\\vspace{2.5\\baselineskip}\n\nB dois.\n");
        let vb = parse(&v2);
        let q: Vec<&Block> = vb.iter().filter(|x| x.kind == "para").collect();
        let (v3, _) = set_vspace(&v2, q[2].start, q[2].end, "before", 0.0).unwrap();
        assert_eq!(v3, s);
        // largura de bloco
        let (w, _) = block_width(s, p[1].start, p[1].end, 0.6).unwrap();
        assert!(w.contains("\\begin{minipage}[t]{0.60\\linewidth}\n\\vspace{0pt}\n\nB dois.\n\\end{minipage}\\par\n"), "{}", w);
        let wb = parse(&w);
        let r = &rows(&w, &wb)[0];
        let (w2, _) = block_width(&w, r.start, r.end, 1.0).unwrap();
        assert_eq!(w2, s);
    }

    #[test]
    fn image_align() {
        let s = "A um.\n\n\\imagem[0.5]{x}{d}{c}\n\nB dois.\n";
        let b = parse(s);
        let p: Vec<&Block> = b.iter().filter(|x| x.kind == "para").collect();
        let (r, _) = img_align(s, p[1].start, p[1].end, "right", "\\imagem", "\\imagemH", 0.92).unwrap();
        assert!(r.contains("\\par\\noindent\\hfill\n\\begin{minipage}[t]{0.50\\linewidth}\n\\vspace{0pt}\n\n\\imagemH[1]{x}{d}{c}\n\\end{minipage}\\par\n"), "{}", r);
        // largura muda e continua à direita
        let rb = parse(&r);
        let row = &rows(&r, &rb)[0];
        assert!(row.right);
        let (r2, _) = set_widths(&r, row.start, &[0.4]).unwrap();
        assert!(r2.contains("\\par\\noindent\\hfill\n\\begin{minipage}[t]{0.40\\linewidth}"), "{}", r2);
        // de volta ao centro
        let rb = parse(&r2);
        let row = &rows(&r2, &rb)[0];
        let (r3, _) = img_align(&r2, row.start, row.end, "center", "\\imagemH", "\\imagemH", 0.92).unwrap();
        assert_eq!(r3, "A um.\n\n\\imagemH[0.4]{x}{d}{c}\n\nB dois.\n");
    }

    #[test]
    fn width_option() {
        let s = "\\newcommand{\\imagem}[3]{%\n\\includegraphics[width=0.92\\linewidth]{imagens/#1.png}\\caption{#3}}\n";
        let o = add_width_option(s, "imagem").unwrap();
        assert_eq!(o, "\\newcommand{\\imagem}[4][0.92]{%\n\\includegraphics[width=#1\\linewidth]{imagens/#2.png}\\caption{#4}}\n");
    }
}
