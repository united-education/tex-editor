mod build;
mod latex;
mod ops;
mod styles;

use build::{Builds, LineMap, NO_FILE};
use latex::{flat_text, parse, rebuild, Block, Inline};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::io::Read;
use std::path::{Component, Path, PathBuf};
use std::time::{Duration, Instant, SystemTime};
use tiny_http::{Header, Method, Request, Response, Server};

const INDEX: &str = include_str!("../web/index.html");
const APP_JS: &str = include_str!("../web/app.js");
const APP_CSS: &str = include_str!("../web/app.css");
const PAGES_JS: &str = include_str!("../web/pages.js");
const LAYOUT_JS: &str = include_str!("../web/layout.js");

type R = Result<Value, (u16, String)>;

fn bad(m: impl Into<String>) -> (u16, String) {
    (400, m.into())
}

struct Doc {
    abs: PathBuf,
    rel: String,
    src: String,
    disk: String,
    crlf: bool,
    bom: bool,
    blocks: Vec<Block>,
    undo: Vec<(String, usize)>,
    redo: Vec<(String, usize)>,
    last_edit: Option<(usize, Instant)>,
    backed_up: bool,
    styles: Value,
}

fn decode(raw: Vec<u8>) -> Result<(String, bool, bool), String> {
    let mut t = String::from_utf8(raw).map_err(|_| "O arquivo não está em UTF-8.".to_string())?;
    let bom = t.starts_with('\u{feff}');
    if bom {
        t.remove(0);
    }
    let crlf = t.contains("\r\n");
    if crlf {
        t = t.replace("\r\n", "\n");
    }
    Ok((t, crlf, bom))
}

impl Doc {
    fn load(root: &Path, rel: &str) -> Result<Doc, String> {
        let abs = root.join(rel);
        let raw = std::fs::read(&abs).map_err(|e| format!("Não consegui abrir {}: {}", rel, e))?;
        let (src, crlf, bom) = decode(raw)?;
        let mut d = Doc {
            styles: styles::collect(&abs, root),
            abs,
            rel: rel.to_string(),
            disk: src.clone(),
            src,
            crlf,
            bom,
            blocks: vec![],
            undo: vec![],
            redo: vec![],
            last_edit: None,
            backed_up: false,
        };
        d.reparse(root);
        Ok(d)
    }

    fn reparse(&mut self, root: &Path) {
        self.blocks = parse(&self.src);
        let dir = self.abs.parent().unwrap_or(root).to_path_buf();
        for b in self.blocks.iter_mut() {
            if b.kind == "cmd" {
                if let Some(t) = b.target.clone() {
                    let t = if t.ends_with(".tex") { t } else { format!("{}.tex", t) };
                    let found = [dir.join(&t), root.join(&t)].into_iter().find(|p| p.is_file());
                    b.target = found.and_then(|p| p.strip_prefix(root).ok().map(|r| r.to_string_lossy().replace('\\', "/")));
                }
            }
        }
    }

    fn kinds(&self) -> Vec<&'static str> {
        self.blocks.iter().map(|b| b.kind).collect()
    }

    fn commit(&mut self, root: &Path, new_src: String, at: usize, coalesce: Option<usize>) {
        if new_src == self.src {
            return;
        }
        let push = match (coalesce, self.last_edit) {
            (Some(id), Some((lid, t))) => !(id == lid && t.elapsed() < Duration::from_secs(4)),
            _ => true,
        };
        if push {
            self.undo.push((self.src.clone(), at));
            if self.undo.len() > 500 {
                self.undo.remove(0);
            }
        }
        self.redo.clear();
        self.last_edit = coalesce.map(|id| (id, Instant::now()));
        self.src = new_src;
        self.reparse(root);
    }

    /// Bloco de texto e deslocamento (em caracteres exibidos) do cursor.
    fn focus(&self, c: usize) -> Value {
        for b in &self.blocks {
            if !b.is_text() || b.locked {
                continue;
            }
            let (cs, ce) = b.content_range();
            if cs <= c && c <= ce {
                let off = latex::char_positions(&b.inl).iter().filter(|&&p| p + cs < c).count();
                return json!({"id": b.id, "offset": off});
            }
        }
        for b in &self.blocks {
            if b.is_text() && !b.locked && b.content_range().0 >= c {
                return json!({"id": b.id, "offset": 0});
            }
        }
        Value::Null
    }

    fn state(&self) -> Value {
        json!({
            "path": self.rel,
            "blocks": self.blocks,
            "dirty": self.src != self.disk,
            "can_undo": !self.undo.is_empty(),
            "can_redo": !self.redo.is_empty(),
            "crlf": self.crlf,
            "locked": self.blocks.iter().filter(|b| b.locked).count(),
        })
    }
}

struct App {
    root: PathBuf,
    docs: HashMap<String, Doc>,
    token: String,
    initial: Option<String>,
    builds: Builds,
    main_choice: HashMap<String, String>,
}

fn safe_rel(rel: &str) -> Result<String, (u16, String)> {
    let p = Path::new(rel);
    if p.is_absolute() || p.components().any(|c| !matches!(c, Component::Normal(_))) {
        return Err(bad("caminho inválido"));
    }
    Ok(rel.replace('\\', "/"))
}

fn s_field<'a>(v: &'a Value, k: &str) -> Result<&'a str, (u16, String)> {
    v.get(k).and_then(|x| x.as_str()).ok_or_else(|| bad(format!("faltou {}", k)))
}
fn u_field(v: &Value, k: &str) -> Result<usize, (u16, String)> {
    v.get(k).and_then(|x| x.as_u64()).map(|x| x as usize).ok_or_else(|| bad(format!("faltou {}", k)))
}

fn leading_ws(s: &str) -> &str {
    let n = s.len() - s.trim_start_matches([' ', '\t']).len();
    &s[..n]
}

fn walk(root: &Path, dir: &Path, out: &mut Vec<String>) {
    if out.len() > 5000 {
        return;
    }
    let Ok(rd) = std::fs::read_dir(dir) else { return };
    let mut ents: Vec<_> = rd.flatten().collect();
    ents.sort_by_key(|e| e.file_name());
    for e in ents {
        let p = e.path();
        let name = e.file_name().to_string_lossy().to_string();
        if name.starts_with('.') || name == "node_modules" || name == "target" {
            continue;
        }
        if p.is_dir() {
            walk(root, &p, out);
        } else if name.ends_with(".tex") {
            if let Ok(r) = p.strip_prefix(root) {
                out.push(r.to_string_lossy().replace('\\', "/"));
            }
        }
    }
}

/// Diff de linhas (unificado, com 2 linhas de contexto).
fn line_diff(a: &str, b: &str) -> String {
    let x: Vec<&str> = a.split('\n').collect();
    let y: Vec<&str> = b.split('\n').collect();
    let mut p = 0;
    while p < x.len() && p < y.len() && x[p] == y[p] {
        p += 1;
    }
    let mut s = 0;
    while s < x.len() - p && s < y.len() - p && x[x.len() - 1 - s] == y[y.len() - 1 - s] {
        s += 1;
    }
    let xm = &x[p..x.len() - s];
    let ym = &y[p..y.len() - s];
    // LCS das linhas do miolo
    let (n, m) = (xm.len(), ym.len());
    let mut ops: Vec<(char, &str)> = vec![];
    if n * m <= 25_000_000 {
        let w = m + 1;
        let mut dp = vec![0u32; (n + 1) * w];
        for i in (0..n).rev() {
            for j in (0..m).rev() {
                dp[i * w + j] = if xm[i] == ym[j] { dp[(i + 1) * w + j + 1] + 1 } else { dp[(i + 1) * w + j].max(dp[i * w + j + 1]) };
            }
        }
        let (mut i, mut j) = (0, 0);
        while i < n || j < m {
            if i < n && j < m && xm[i] == ym[j] {
                ops.push((' ', xm[i]));
                i += 1;
                j += 1;
            } else if j < m && (i == n || dp[i * w + j + 1] >= dp[(i + 1) * w + j]) {
                ops.push(('+', ym[j]));
                j += 1;
            } else {
                ops.push(('-', xm[i]));
                i += 1;
            }
        }
    } else {
        ops.extend(xm.iter().map(|l| ('-', *l)));
        ops.extend(ym.iter().map(|l| ('+', *l)));
    }
    let mut all: Vec<(char, &str, usize)> = vec![];
    for (k, l) in x[..p].iter().enumerate() {
        all.push((' ', l, k + 1));
    }
    let mut ln = p;
    for (o, l) in ops {
        if o != '+' {
            ln += 1;
        }
        all.push((o, l, ln));
    }
    for l in &x[x.len() - s..] {
        ln += 1;
        all.push((' ', l, ln));
    }
    let keep: Vec<bool> = (0..all.len())
        .map(|i| {
            let lo = i.saturating_sub(2);
            let hi = (i + 3).min(all.len());
            all[lo..hi].iter().any(|e| e.0 != ' ')
        })
        .collect();
    let mut out = String::new();
    let mut gap = false;
    for (i, (o, l, n)) in all.iter().enumerate() {
        if keep[i] {
            if gap && !out.is_empty() {
                out.push_str("@@\n");
            }
            gap = false;
            if *o == ' ' {
                out.push_str(&format!("{:>5}  {}\n", n, l));
            } else {
                out.push_str(&format!("{:>5}{} {}\n", if *o == '-' { n.to_string() } else { String::new() }, o, l));
            }
        } else {
            gap = true;
        }
    }
    out
}

fn backup_dir() -> PathBuf {
    if let Ok(l) = std::env::var("LOCALAPPDATA") {
        return PathBuf::from(l).join("tex-editor").join("backups");
    }
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
    PathBuf::from(home).join(".local/share/tex-editor/backups")
}

fn now_stamp() -> String {
    let s = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    s.to_string()
}

fn write_atomic(path: &Path, text: &str, crlf: bool, bom: bool) -> Result<(), (u16, String)> {
    let mut out = if crlf { text.replace('\n', "\r\n") } else { text.to_string() };
    if bom {
        out.insert(0, '\u{feff}');
    }
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| bad(e.to_string()))?;
    }
    let tmp = path.with_extension("tex.tex-editor-tmp");
    std::fs::write(&tmp, out.as_bytes()).map_err(|e| bad(e.to_string()))?;
    std::fs::rename(&tmp, path).map_err(|e| bad(e.to_string()))
}

fn save_doc(d: &mut Doc, force: bool) -> Result<(), (u16, String)> {
    let on_disk = std::fs::read(&d.abs).map_err(|e| bad(e.to_string()))?;
    let (disk_now, _, _) = decode(on_disk.clone()).map_err(bad)?;
    if disk_now != d.disk && !force {
        return Err((409, format!("{} foi alterado fora do editor desde que foi aberto.", d.rel)));
    }
    if !d.backed_up {
        let dir = backup_dir();
        let _ = std::fs::create_dir_all(&dir);
        let name = d.rel.replace('/', "__");
        let _ = std::fs::write(dir.join(format!("{}.{}", name, now_stamp())), &on_disk);
        d.backed_up = true;
    }
    write_atomic(&d.abs, &d.src, d.crlf, d.bom)?;
    d.disk = d.src.clone();
    Ok(())
}

#[derive(Default)]
struct Flat {
    inlined: Vec<String>,
    missing: Vec<String>,
}

impl App {
    fn rel_of(&self, p: &Path) -> String {
        p.strip_prefix(&self.root).map(|r| r.to_string_lossy().replace('\\', "/")).unwrap_or_else(|_| p.to_string_lossy().to_string())
    }

    /// Conteúdo atual de um arquivo: a versão do editor (com o que ainda não
    /// foi salvo) se ele estiver aberto, senão a do disco.
    fn current_src(&self, p: &Path) -> Option<String> {
        if let Some(d) = self.docs.get(&self.rel_of(p)) {
            return Some(d.src.clone());
        }
        std::fs::read(p).ok().and_then(|r| decode(r).ok()).map(|x| x.0)
    }

    /// Troca cada \input/\include pelo conteúdo do arquivo, recursivamente,
    /// anotando em `map` de que arquivo e linha veio cada linha da saída.
    /// O LaTeX resolve os caminhos a partir da pasta do arquivo principal.
    #[allow(clippy::too_many_arguments)]
    fn flatten(&self, src: &str, rel: &str, file_dir: &Path, main_dir: &Path, seen: &mut Vec<PathBuf>, rep: &mut Flat, out: &mut String, map: &mut LineMap) {
        let fi = map.file_idx(rel);
        let mut line: u32 = 1;
        for b in parse(src) {
            let nl = b.src.matches('\n').count() as u32;
            if b.kind == "cmd" && (b.env == "input" || b.env == "include") {
                if let Some(t) = b.target.as_deref().map(str::trim) {
                    let name = if t.ends_with(".tex") { t.to_string() } else { format!("{}.tex", t) };
                    let found = [main_dir.join(&name), file_dir.join(&name), self.root.join(&name)].into_iter().find(|p| p.is_file());
                    match found {
                        Some(p) if seen.len() < 30 && !seen.contains(&p) => {
                            let mut inner = self.current_src(&p).unwrap_or_default();
                            if let Some(k) = inner.find("\n\\endinput") {
                                inner.truncate(k + 1);
                            }
                            let irel = self.rel_of(&p);
                            rep.inlined.push(irel.clone());
                            let ws = leading_ws(&b.src).to_string();
                            let include = b.env == "include";
                            if include {
                                map.emit(out, &format!("{}\\clearpage\n", ws), NO_FILE, 0);
                            }
                            map.emit(out, &format!("{}% ===== início de {} =====\n", ws, irel), NO_FILE, 0);
                            seen.push(p.clone());
                            let dir = p.parent().unwrap_or(main_dir).to_path_buf();
                            self.flatten(&inner, &irel, &dir, main_dir, seen, rep, out, map);
                            seen.pop();
                            if !out.ends_with('\n') {
                                map.emit(out, "\n", NO_FILE, 0);
                            }
                            map.emit(out, &format!("{}% ===== fim de {} =====\n", ws, irel), NO_FILE, 0);
                            if include {
                                map.emit(out, &format!("{}\\clearpage\n", ws), NO_FILE, 0);
                            }
                            if !b.src.ends_with('\n') {
                                // o comando não terminava a linha: mantém o que vinha depois na mesma posição
                                out.pop();
                            }
                            line += nl;
                            continue;
                        }
                        Some(p) => rep.missing.push(format!("{} (inclusão circular)", self.rel_of(&p))),
                        None => rep.missing.push(t.to_string()),
                    }
                }
            }
            map.emit(out, &b.src, fi, line);
            line += nl;
        }
    }

    /// Texto achatado do arquivo principal `main` com as versões do editor.
    fn flat_job(&self, main: &str) -> Result<(String, LineMap, Flat), String> {
        let abs = self.root.join(main);
        let src = self.current_src(&abs).ok_or_else(|| format!("não consegui ler {}", main))?;
        let dir = abs.parent().unwrap_or(&self.root).to_path_buf();
        let mut rep = Flat::default();
        let mut map = LineMap::default();
        let mut out = String::new();
        let mut seen = vec![abs.clone()];
        self.flatten(&src, main, &dir, &dir, &mut seen, &mut rep, &mut out, &mut map);
        Ok((out, map, rep))
    }

    /// Arquivos com \documentclass (na pasta do documento e acima) que
    /// incluem `rel`, do menor para o maior.
    fn mains_for(&self, rel: &str) -> Vec<String> {
        let mut cands: Vec<(usize, String)> = vec![];
        let mut dir = self.root.join(rel).parent().map(|p| p.to_path_buf());
        while let Some(d) = dir {
            if !d.starts_with(&self.root) {
                break;
            }
            if let Ok(rd) = std::fs::read_dir(&d) {
                for e in rd.flatten() {
                    let p = e.path();
                    if p.extension().map_or(true, |x| x != "tex") {
                        continue;
                    }
                    let Some(src) = self.current_src(&p) else { continue };
                    if !src.contains("\\documentclass") {
                        continue;
                    }
                    let mrel = self.rel_of(&p);
                    if mrel == rel {
                        cands.push((0, mrel));
                        continue;
                    }
                    if let Ok((_, map, rep)) = self.flat_job(&mrel) {
                        let _ = map;
                        if rep.inlined.iter().any(|x| x == rel) {
                            cands.push((rep.inlined.len(), mrel));
                        }
                    }
                }
            }
            if d == self.root {
                break;
            }
            dir = d.parent().map(|p| p.to_path_buf());
        }
        cands.sort();
        cands.dedup_by(|a, b| a.1 == b.1);
        cands.into_iter().map(|c| c.1).collect()
    }

    fn build_status(&self, main: &str) -> R {
        let g = self.builds.lock().unwrap();
        let Some(s) = g.get(main) else { return Ok(json!({"main": main, "running": false, "seq": 0, "ok_seq": 0})) };
        let ok_seq = s.ok.as_ref().map_or(0, |r| r.seq);
        Ok(json!({
            "main": main,
            "running": s.running,
            "seq": s.seq,
            "ok_seq": ok_seq,
            "pages": s.ok.as_ref().map_or(0, |r| r.pages),
            "ms": s.ok.as_ref().map_or(0, |r| r.ms as u64),
            "failed": s.failed_seq > ok_seq,
            "errors": s.errors,
            "log": s.log,
            "tectonic": build::tectonic().is_some(),
        }))
    }

    /// Macros que mostram imagem (\newcommand com \includegraphics no corpo):
    /// quantos argumentos, se aceitam largura opcional e qual argumento é o arquivo.
    fn img_macros(&mut self, rel: &str) -> Value {
        let mut files: Vec<String> = vec![rel.to_string()];
        if let Some(m) = self.main_of(rel) {
            if let Ok((_, map, _)) = self.flat_job(&m) {
                files.extend(map.files);
            }
        }
        files.dedup();
        let mut out = serde_json::Map::new();
        for f in files {
            let Some(src) = self.current_src(&self.root.join(&f)) else { continue };
            let p = latex::P::new(&src);
            let mut from = 0;
            while let Some(k) = src[from..].find("\\newcommand{\\") {
                let a = from + k;
                from = a + 13;
                let Some(ne) = src[a + 13..].find('}') else { continue };
                let name = src[a + 13..a + 13 + ne].to_string();
                let mut j = a + 13 + ne + 1;
                let mut nargs = 0u32;
                let mut default: Option<String> = None;
                if src[j..].starts_with('[') {
                    if let Some(c) = src[j..].find(']') {
                        nargs = src[j + 1..j + c].trim().parse().unwrap_or(0);
                        j += c + 1;
                    }
                }
                if src[j..].starts_with('[') {
                    if let Some(c) = src[j..].find(']') {
                        default = Some(src[j + 1..j + c].to_string());
                        j += c + 1;
                    }
                }
                if !src[j..].starts_with('{') {
                    continue;
                }
                let Some(be) = p.match_brace(j, src.len()) else { continue };
                let body = &src[j..be];
                let Some(ig) = body.find("\\includegraphics") else { continue };
                // argumento usado no nome do arquivo
                let after = &body[ig..];
                let fa = after.find(']').map(|x| x + 1).unwrap_or(16);
                let file_arg = after[fa..].find('#').and_then(|h| after[fa + h + 1..].chars().next()).and_then(|c| c.to_digit(10));
                let opt = default.is_some();
                let id = file_arg.map(|k| if opt { k as i64 - 2 } else { k as i64 - 1 });
                let width = default.as_deref().and_then(|d| d.trim().parse::<f64>().ok()).or_else(|| {
                    let k = body.find("\\linewidth")?;
                    let num: String = body[..k].chars().rev().take_while(|c| c.is_ascii_digit() || *c == '.').collect::<Vec<_>>().into_iter().rev().collect();
                    num.parse().ok()
                });
                // argumentos que só aparecem no "senão" de \IfFileExists (o texto
                // do marcador de imagem que falta): somem do PDF quando a imagem existe
                let mut hidden: Vec<i64> = vec![];
                if let Some(ix) = body.find("\\IfFileExists") {
                    let bp = latex::P::new(body);
                    let mut j = ix + "\\IfFileExists".len();
                    let mut parts = vec![];
                    for _ in 0..3 {
                        if !body[j..].starts_with('{') {
                            break;
                        }
                        let Some(k) = bp.match_brace(j, body.len()) else { break };
                        parts.push((j, k));
                        j = k;
                        // pula espaços e comentários (% até o fim da linha) entre os argumentos
                        loop {
                            if body[j..].starts_with('%') {
                                j += body[j..].find('\n').map_or(body.len() - j, |x| x + 1);
                            } else if body[j..].starts_with([' ', '\n', '\t', '\r']) {
                                j += 1;
                            } else {
                                break;
                            }
                        }
                    }
                    if parts.len() == 3 {
                        let (fa, fb) = parts[2];
                        let rest = format!("{}{}", &body[..fa], &body[fb..]);
                        for k in 1..=nargs {
                            let tag = format!("#{}", k);
                            if body[fa..fb].contains(&tag) && !rest.contains(&tag) {
                                hidden.push(if opt { k as i64 - 2 } else { k as i64 - 1 });
                            }
                        }
                    }
                }
                // como cada argumento sai no PDF: {\small #3} -> pequeno, \caption{#3} -> legenda
                let skip = else_range(body);
                let mut sty = serde_json::Map::new();
                for k in 1..=nargs {
                    let tag = format!("#{}", k);
                    let mut from = 0;
                    let pos = loop {
                        let Some(i) = body[from..].find(&tag).map(|i| i + from) else { break None };
                        from = i + 1;
                        if skip.map_or(false, |(a, b)| i >= a && i < b) {
                            continue;
                        }
                        break Some(i);
                    };
                    let Some(pos) = pos else { continue };
                    // grupo { ... } que contém o argumento
                    let mut d = 0i32;
                    let mut g = None;
                    for (j, ch) in body[..pos].char_indices().rev() {
                        match ch {
                            '}' => d += 1,
                            '{' if d == 0 => {
                                g = Some(j);
                                break;
                            }
                            '{' => d -= 1,
                            _ => {}
                        }
                    }
                    let Some(g) = g else { continue };
                    let group = &body[g + 1..pos];
                    let before = body[..g].trim_end();
                    let mut s = serde_json::Map::new();
                    if before.ends_with("\\caption") {
                        s.insert("caption".into(), json!(true));
                    }
                    for z in ["footnotesize", "small", "scriptsize", "large"] {
                        if group.contains(&format!("\\{}", z)) {
                            s.insert("size".into(), json!(z));
                            break;
                        }
                    }
                    if group.contains("\\itshape") {
                        s.insert("italic".into(), json!(true));
                    }
                    if !s.is_empty() {
                        let ai = if opt { k as i64 - 2 } else { k as i64 - 1 };
                        sty.insert(ai.to_string(), Value::Object(s));
                    }
                }
                // \label{fig:#1}: o número da figura vem do .aux
                let label = body.find("\\label{").and_then(|a| body[a + 7..].find('}').map(|e| body[a + 7..a + 7 + e].to_string()));
                out.insert(format!("\\{}", name), json!({"file": f, "nargs": nargs, "opt": opt, "id": id, "default": width.unwrap_or(1.0), "hidden": hidden, "argsty": sty, "label": label, "labelopt": opt, "centered": body.contains("\\begin{center}") || body.contains("\\centering"), "float": body.contains("\\begin{figure") || body.contains("\\begin{table")}));
                from = be;
            }
        }
        Value::Object(out)
    }

    fn main_of(&mut self, rel: &str) -> Option<String> {
        if let Some(m) = self.main_choice.get(rel) {
            return Some(m.clone());
        }
        let m = self.mains_for(rel).into_iter().next()?;
        self.main_choice.insert(rel.to_string(), m.clone());
        Some(m)
    }

    fn doc(&mut self, body: &Value) -> Result<&mut Doc, (u16, String)> {
        let rel = safe_rel(s_field(body, "path")?)?;
        if !self.docs.contains_key(&rel) {
            let d = Doc::load(&self.root, &rel).map_err(bad)?;
            self.docs.insert(rel.clone(), d);
        }
        Ok(self.docs.get_mut(&rel).unwrap())
    }

    fn api(&mut self, name: &str, body: Value) -> R {
        let root = self.root.clone();
        match name {
            "init" => {
                let mut files = vec![];
                walk(&root, &root, &mut files);
                Ok(json!({"root": root.to_string_lossy(), "files": files, "initial": self.initial}))
            }
            "files" => {
                let mut files = vec![];
                walk(&root, &root, &mut files);
                let dirty: Vec<&String> = self.docs.iter().filter(|(_, d)| d.src != d.disk).map(|(k, _)| k).collect();
                Ok(json!({"files": files, "dirty": dirty}))
            }
            "open" => {
                let rel = safe_rel(s_field(&body, "path")?)?;
                let d = self.doc(&body)?;
                let mut st = d.state();
                st["styles"] = d.styles.clone();
                st["macros"] = self.img_macros(&rel);
                Ok(st)
            }
            "edit" => {
                let structural = body.get("structural").and_then(|x| x.as_bool()).unwrap_or(false);
                let id = u_field(&body, "id")?;
                let mut inl: Vec<Inline> = serde_json::from_value(body.get("inl").cloned().unwrap_or(json!([]))).map_err(|e| bad(e.to_string()))?;
                let d = self.doc(&body)?;
                let b = d.blocks.get(id).filter(|b| b.is_text() && !b.locked).ok_or_else(|| bad("bloco não editável"))?.clone();
                let par = match b.kind {
                    "item" => format!("\n{}\\item ", leading_ws(&b.pre)),
                    "para" => "\n\n".to_string(),
                    _ => String::new(),
                };
                let (content, caret) = rebuild(&b.inl, &mut inl, &par);
                let (cs, ce) = b.content_range();
                let new_src = format!("{}{}{}", &d.src[..cs], content, &d.src[ce..]);
                let sent = flat_text(&inl);
                let kinds = d.kinds();
                d.commit(&root, new_src, cs, if structural { None } else { Some(id) });
                let changed = kinds != d.kinds();
                let same = !changed && d.blocks.get(id).map_or(false, |nb| flat_text(&nb.inl) == sent);
                let mut st = d.state();
                st["changed"] = json!(changed);
                st["same"] = json!(same);
                if let Some(c) = caret {
                    st["focus"] = d.focus(cs + c);
                }
                Ok(st)
            }
            "insert" => {
                let id = u_field(&body, "id")?;
                let wh = s_field(&body, "where")?.to_string();
                let mut inl: Vec<Inline> = serde_json::from_value(body.get("inl").cloned().unwrap_or(json!([]))).map_err(|e| bad(e.to_string()))?;
                let d = self.doc(&body)?;
                let b = d.blocks.get(id).filter(|b| b.is_text()).ok_or_else(|| bad("bloco inválido"))?.clone();
                let par = if b.kind == "item" { format!("\n{}\\item ", leading_ws(&b.pre)) } else { "\n\n".to_string() };
                let (content, caret) = rebuild(&[], &mut inl, &par);
                if content.trim().is_empty() {
                    return Ok(d.state());
                }
                let (_, ce) = b.content_range();
                let ws = leading_ws(&b.pre).to_string();
                let (pos, head, tail) = match (b.kind, wh.as_str()) {
                    ("item", "after") => (ce, format!("\n{}\\item ", ws), String::new()),
                    ("item", _) => (b.start, format!("{}\\item ", ws), "\n".to_string()),
                    ("heading", "after") => {
                        let tail = if d.src[b.end..].starts_with('\n') { "\n" } else { "\n\n" };
                        (b.end, "\n".to_string(), tail.to_string())
                    }
                    (_, "after") => (ce, "\n\n".to_string(), String::new()),
                    _ => (b.start, String::new(), "\n\n".to_string()),
                };
                let new_src = format!("{}{}{}{}{}", &d.src[..pos], head, content, tail, &d.src[pos..]);
                let caret_abs = pos + head.len() + caret.unwrap_or(content.len());
                d.commit(&root, new_src, pos, None);
                let mut st = d.state();
                st["focus"] = d.focus(caret_abs);
                Ok(st)
            }
            "merge" => {
                let id = u_field(&body, "id")?;
                let d = self.doc(&body)?;
                let cur = d.blocks.get(id).filter(|b| b.is_text() && !b.locked).ok_or_else(|| bad("bloco inválido"))?.clone();
                let mut p = id;
                let prev = loop {
                    if p == 0 {
                        break None;
                    }
                    p -= 1;
                    let b = &d.blocks[p];
                    if b.kind != "blank" {
                        break Some(b.clone());
                    }
                };
                let mut st;
                match prev {
                    Some(pb) if pb.is_text() && !pb.locked && (pb.kind == cur.kind && pb.kind != "heading") => {
                        let (_, pe) = pb.content_range();
                        let (cs, ce) = cur.content_range();
                        let sep = if pb.content_range().0 < pe && cs < ce { " " } else { "" };
                        let new_src = format!("{}{}{}", &d.src[..pe], sep, &d.src[cs..]);
                        d.commit(&root, new_src, pe, None);
                        st = d.state();
                        st["focus"] = d.focus(pe);
                    }
                    Some(pb) if pb.is_text() && !pb.locked => {
                        st = d.state();
                        st["focus"] = d.focus(pb.content_range().1);
                    }
                    _ => st = d.state(),
                }
                Ok(st)
            }
            "raw" => {
                let id = u_field(&body, "id")?;
                let mut src = s_field(&body, "src")?.replace("\r\n", "\n");
                let d = self.doc(&body)?;
                let b = d.blocks.get(id).ok_or_else(|| bad("bloco inválido"))?.clone();
                if b.src.ends_with('\n') && !src.ends_with('\n') && !src.is_empty() {
                    src.push('\n');
                }
                let new_src = format!("{}{}{}", &d.src[..b.start], src, &d.src[b.end..]);
                d.commit(&root, new_src, b.start, None);
                let mut st = d.state();
                st["focus"] = d.focus(b.start);
                Ok(st)
            }
            "retype" => {
                let id = u_field(&body, "id")?;
                let to = s_field(&body, "to")?.to_string();
                let d = self.doc(&body)?;
                let b = d.blocks.get(id).filter(|b| b.is_text() && !b.locked && b.kind != "item").ok_or_else(|| bad("bloco inválido"))?.clone();
                let (cs, ce) = b.content_range();
                let content = d.src[cs..ce].to_string();
                let ws = leading_ws(&b.pre).to_string();
                let new_block = if to == "para" {
                    if b.kind == "para" {
                        return Ok(d.state());
                    }
                    format!("{}{}\n", ws, content)
                } else {
                    let tail = if b.kind == "heading" { b.post.clone() } else { format!("}}{}", b.post) };
                    format!("{}\\{}{{{}{}", ws, to, content, tail)
                };
                let caret = b.start + new_block.find(&content).unwrap_or(0);
                let new_src = format!("{}{}{}", &d.src[..b.start], new_block, &d.src[b.end..]);
                d.commit(&root, new_src, b.start, None);
                let mut st = d.state();
                st["focus"] = d.focus(caret);
                Ok(st)
            }
            "setsource" => {
                let src = s_field(&body, "src")?.replace("\r\n", "\n");
                let d = self.doc(&body)?;
                d.commit(&root, src, 0, None);
                Ok(d.state())
            }
            "undo" | "redo" => {
                let d = self.doc(&body)?;
                let (from, to) = if name == "undo" { (&mut d.undo, &mut d.redo) } else { (&mut d.redo, &mut d.undo) };
                if let Some((src, at)) = from.pop() {
                    to.push((std::mem::replace(&mut d.src, src), at));
                    d.last_edit = None;
                    d.reparse(&root);
                    let mut st = d.state();
                    st["focus"] = d.focus(at);
                    return Ok(st);
                }
                Ok(d.state())
            }
            "check" => {
                let d = self.doc(&body)?;
                let changed = match std::fs::read(&d.abs).ok().and_then(|r| decode(r).ok()) {
                    Some((t, _, _)) => t != d.disk,
                    None => true,
                };
                Ok(json!({"changed": changed, "dirty": d.src != d.disk}))
            }
            "reload" => {
                let rel = safe_rel(s_field(&body, "path")?)?;
                self.docs.remove(&rel);
                let d = self.doc(&body)?;
                let mut st = d.state();
                st["styles"] = d.styles.clone();
                Ok(st)
            }
            "diff" => {
                let d = self.doc(&body)?;
                Ok(json!({"diff": line_diff(&d.disk, &d.src)}))
            }
            "save" => {
                let force = body.get("force").and_then(|x| x.as_bool()).unwrap_or(false);
                let d = self.doc(&body)?;
                save_doc(d, force)?;
                Ok(d.state())
            }
            "saveall" => {
                let force = body.get("force").and_then(|x| x.as_bool()).unwrap_or(false);
                let mut saved = vec![];
                let mut conflicts = vec![];
                let mut errors = vec![];
                let keys: Vec<String> = self.docs.iter().filter(|(_, d)| d.src != d.disk).map(|(k, _)| k.clone()).collect();
                for k in keys {
                    let d = self.docs.get_mut(&k).unwrap();
                    match save_doc(d, force) {
                        Ok(()) => saved.push(k),
                        Err((409, _)) => conflicts.push(k),
                        Err((_, m)) => errors.push(format!("{}: {}", k, m)),
                    }
                }
                let mut st = match self.doc(&body) {
                    Ok(d) => d.state(),
                    Err(_) => json!({}),
                };
                st["saved"] = json!(saved);
                st["conflicts"] = json!(conflicts);
                st["errors"] = json!(errors);
                Ok(st)
            }
            "mains" => {
                let rel = safe_rel(s_field(&body, "path")?)?;
                let mains = self.mains_for(&rel);
                let main = self.main_of(&rel);
                Ok(json!({"mains": mains, "main": main}))
            }
            "setmain" => {
                let rel = safe_rel(s_field(&body, "path")?)?;
                let main = safe_rel(s_field(&body, "main")?)?;
                self.main_choice.insert(rel, main.clone());
                Ok(json!({"main": main}))
            }
            "build" => {
                let rel = safe_rel(s_field(&body, "path")?)?;
                self.doc(&body)?;
                let Some(main) = self.main_of(&rel) else {
                    return Ok(json!({"main": null, "nomain": true, "running": false, "failed": true, "errors": [{"msg": "Nenhum arquivo com \\documentclass inclui este arquivo, então não dá para compilar. Abra o arquivo principal ou crie um wrapper."}]}));
                };
                let (text, mut map, _) = self.flat_job(&main).map_err(bad)?;
                // pede ao LaTeX as medidas da página (o editor desenha as páginas com elas)
                let text = format!("{}{}", build::GEOM_PROBE, text);
                map.lines.insert(0, (build::NO_FILE, 0));
                let mut snaps = HashMap::new();
                for f in &map.files {
                    if let Some(s) = self.current_src(&self.root.join(f)) {
                        snaps.insert(f.clone(), s);
                    }
                }
                let main_dir = self.root.join(&main).parent().unwrap_or(&self.root).to_path_buf();
                build::submit(&self.builds, build::Job { main: main.clone(), main_dir, text, map, snapshots: snaps });
                self.build_status(&main)
            }
            "buildstatus" => {
                let rel = safe_rel(s_field(&body, "path")?)?;
                match self.main_of(&rel) {
                    Some(m) => self.build_status(&m),
                    None => Ok(json!({"main": null})),
                }
            }
            "layout" => {
                let rel = safe_rel(s_field(&body, "path")?)?;
                let main = self.main_of(&rel).ok_or_else(|| bad("sem arquivo principal"))?;
                let res = self.builds.lock().unwrap().get(&main).and_then(|s| s.ok.clone());
                let Some(r) = res else { return Ok(json!({"seq": 0})) };
                let d = self.doc(&body)?;
                let stale = r.snapshots.get(&rel).map_or(true, |s| *s != d.src);
                // linhas de cada bloco (no fonte atual)
                let mut starts = vec![0usize];
                for (i, c) in d.src.bytes().enumerate() {
                    if c == b'\n' {
                        starts.push(i + 1);
                    }
                }
                let line_of = |pos: usize| -> u32 { (starts.partition_point(|&s| s <= pos)) as u32 };
                let empty = HashMap::new();
                let hits = r.hits.get(&rel).unwrap_or(&empty);
                let mut blocks = vec![];
                for b in &d.blocks {
                    if b.kind == "blank" || b.end <= b.start {
                        continue;
                    }
                    let (l0, l1) = (line_of(b.start), line_of(b.end - 1));
                    let rs = build::rects_for(hits, l0, l1);
                    if !rs.is_empty() {
                        // linhas impressas em cada página (linhas de base distintas)
                        let mut n: Vec<(u16, Vec<i32>)> = vec![];
                        if let Some(bb) = r.bases.get(&rel) {
                            for l in l0..=l1 {
                                for &(p, y) in bb.get(&l).into_iter().flatten() {
                                    match n.iter_mut().find(|x| x.0 == p) {
                                        Some(x) => {
                                            if !x.1.iter().any(|&v| (v - y).abs() <= 9) {
                                                x.1.push(y)
                                            }
                                        }
                                        None => n.push((p, vec![y])),
                                    }
                                }
                            }
                        }
                        let n: Vec<[u32; 2]> = n.iter().map(|x| [x.0 as u32, x.1.len() as u32]).collect();
                        blocks.push(json!({"id": b.id, "r": rs.iter().map(|x| [x.p as f32, x.x0, x.y0, x.x1, x.y1]).collect::<Vec<_>>(), "n": n}));
                    }
                }
                // ocupação de cada página (todas as fontes)
                let mut fill: Vec<[f32; 2]> = vec![[f32::MAX, 0.0]; r.pages + 1];
                for per in r.hits.values() {
                    for v in per.values() {
                        for x in v {
                            if let Some(f) = fill.get_mut(x.p as usize) {
                                f[0] = f[0].min(x.y0);
                                f[1] = f[1].max(x.y1);
                            }
                        }
                    }
                }
                Ok(json!({"seq": r.seq, "main": main, "pages": r.pages, "page_w": r.page_w, "page_h": r.page_h, "stale": stale, "blocks": blocks, "geom": r.geom, "labels": r.labels, "fill": fill.iter().skip(1).map(|f| if f[0] == f32::MAX { json!(null) } else { json!(f) }).collect::<Vec<_>>(), "ms": r.ms as u64}))
            }
            "replace" => {
                let s = u_field(&body, "s")?;
                let e = u_field(&body, "e")?;
                let mut text = s_field(&body, "text")?.replace("\r\n", "\n");
                let d = self.doc(&body)?;
                if s > e || e > d.src.len() || !d.blocks.iter().any(|b| b.start == s) || !d.blocks.iter().any(|b| b.end == e) {
                    return Err(bad("trecho inválido"));
                }
                if d.src[s..e].ends_with('\n') && !text.ends_with('\n') && !text.is_empty() {
                    text.push('\n');
                }
                let new = format!("{}{}{}", &d.src[..s], text, &d.src[e..]);
                d.commit(&root, new, s, None);
                let mut st = d.state();
                st["focus"] = d.focus(s);
                Ok(st)
            }
            "move" | "side" | "widths" | "unrow" | "addcol" | "pagebreak" | "delunit" | "dupunit" | "decl" | "vspace" | "blockwidth" | "swapcols" | "imgalign" => {
                let g = |k: &str| u_field(&body, k);
                let d = self.doc(&body)?;
                let src = d.src.clone();
                let res = match name {
                    "move" => ops::move_unit(&src, g("s")?, g("e")?, g("to")?),
                    "side" => ops::side_by_side(&src, g("s")?, g("e")?, g("ts")?, g("te")?, body.get("right").and_then(|x| x.as_bool()).unwrap_or(true)),
                    "widths" => {
                        let ws: Vec<f64> = body.get("widths").and_then(|x| x.as_array()).map(|a| a.iter().filter_map(|v| v.as_f64()).collect()).unwrap_or_default();
                        ops::set_widths(&src, g("pos")?, &ws)
                    }
                    "unrow" => ops::unrow(&src, g("pos")?),
                    "addcol" => ops::add_column(&src, g("pos")?),
                    "pagebreak" => ops::page_break(&src, g("pos")?, body.get("cmd").and_then(|x| x.as_str()).filter(|c| *c == "newpage" || *c == "clearpage").unwrap_or("newpage")),
                    "delunit" => ops::delete_unit(&src, g("s")?, g("e")?),
                    "decl" => ops::set_decl(&src, g("s")?, g("e")?, s_field(&body, "kind")?, s_field(&body, "value")?),
                    "vspace" => ops::set_vspace(&src, g("s")?, g("e")?, s_field(&body, "side")?, body.get("lines").and_then(|x| x.as_f64()).unwrap_or(0.0)),
                    "blockwidth" => ops::block_width(&src, g("s")?, g("e")?, body.get("w").and_then(|x| x.as_f64()).unwrap_or(1.0)),
                    "swapcols" => ops::swap_cols(&src, g("pos")?),
                    "imgalign" => ops::img_align(&src, g("s")?, g("e")?, s_field(&body, "side")?, s_field(&body, "name")?, s_field(&body, "fixed")?, body.get("default").and_then(|x| x.as_f64()).unwrap_or(0.92)),
                    _ => ops::duplicate_unit(&src, g("s")?, g("e")?),
                };
                let (new, caret) = res.map_err(bad)?;
                let at = body.get("s").or(body.get("pos")).and_then(|x| x.as_u64()).unwrap_or(0) as usize;
                d.commit(&root, new, at.min(d.src.len()), None);
                let mut st = d.state();
                st["focus"] = d.focus(caret);
                Ok(st)
            }
            "imgopt" => {
                // permite largura por imagem: \newcommand{\imagem}[3] -> [4][0.92]
                let rel = safe_rel(s_field(&body, "path")?)?;
                let mname = s_field(&body, "name")?.trim_start_matches('\\').to_string();
                let head = format!("\\newcommand{{\\{}}}[", mname);
                let mut files: Vec<String> = vec![rel.clone()];
                if let Some(m) = self.main_of(&rel) {
                    if let Ok((_, map, _)) = self.flat_job(&m) {
                        files.extend(map.files);
                    }
                }
                let target = files.into_iter().find(|f| self.current_src(&self.root.join(f)).map_or(false, |s| s.contains(&head)));
                let Some(f) = target else { return Err(bad(format!("Não achei a definição de \\{}.", mname))) };
                let fb = json!({"path": f});
                let d = self.doc(&fb)?;
                let new = ops::add_width_option(&d.src, &mname).map_err(bad)?;
                let changed = new != d.src;
                d.commit(&root, new, 0, None);
                Ok(json!({"file": f, "changed": changed, "macros": self.img_macros(&rel)}))
            }
            "saveas" => {
                let from = safe_rel(s_field(&body, "path")?)?;
                let to = safe_rel(s_field(&body, "to")?.trim())?;
                let mode = s_field(&body, "mode")?.to_string();
                let overwrite = body.get("overwrite").and_then(|x| x.as_bool()).unwrap_or(false);
                if !to.ends_with(".tex") {
                    return Err(bad("O nome tem que terminar em .tex"));
                }
                self.doc(&body)?;
                let dest = root.join(&to);
                if to == from {
                    return Err(bad("Escolha um nome diferente do arquivo atual (para gravar nele, use Salvar)."));
                }
                if dest.exists() && !overwrite {
                    return Err((409, format!("Já existe um arquivo {}.", to)));
                }
                let (src, crlf, bom, abs) = {
                    let d = &self.docs[&from];
                    (d.src.clone(), d.crlf, d.bom, d.abs.clone())
                };
                if mode == "flat" {
                    let mut rep = Flat::default();
                    let dir = abs.parent().unwrap_or(&root).to_path_buf();
                    let mut seen = vec![abs.clone()];
                    let mut text = String::new();
                    let mut map = LineMap::default();
                    self.flatten(&src, &from, &dir, &dir, &mut seen, &mut rep, &mut text, &mut map);
                    write_atomic(&dest, &text, crlf, bom)?;
                    self.docs.remove(&to);
                    let mut files = vec![];
                    walk(&root, &root, &mut files);
                    return Ok(json!({"written": to, "inlined": rep.inlined, "missing": rep.missing, "files": files}));
                }
                // cópia: as edições vão para o arquivo novo; o original fica como está no disco
                write_atomic(&dest, &src, crlf, bom)?;
                self.docs.remove(&from);
                self.docs.remove(&to);
                let d = Doc::load(&root, &to).map_err(bad)?;
                let mut st = d.state();
                st["styles"] = d.styles.clone();
                self.docs.insert(to.clone(), d);
                let mut files = vec![];
                walk(&root, &root, &mut files);
                st["files"] = json!(files);
                Ok(st)
            }
            _ => Err((404, "rota desconhecida".into())),
        }
    }

    fn image(&self, query: &HashMap<String, String>) -> Option<(Vec<u8>, &'static str)> {
        let name = query.get("name")?;
        if name.contains("..") || name.starts_with('/') {
            return None;
        }
        let doc = safe_rel(query.get("doc").map(|s| s.as_str()).unwrap_or("")).ok();
        let base = doc.map(|d| self.root.join(d)).and_then(|p| p.parent().map(|x| x.to_path_buf())).unwrap_or(self.root.clone());
        let mut dirs = vec![base.clone(), base.join("imagens"), base.join("figuras"), base.join("img"), base.join("images")];
        if let Some(pp) = base.parent() {
            dirs.push(pp.join("imagens"));
        }
        dirs.push(self.root.join("imagens"));
        dirs.push(self.root.clone());
        for d in dirs {
            if !d.starts_with(&self.root) {
                continue;
            }
            for ext in ["", ".png", ".jpg", ".jpeg", ".webp", ".gif", ".svg"] {
                let p = d.join(format!("{}{}", name, ext));
                if p.is_file() {
                    let ct = match p.extension().and_then(|e| e.to_str()).unwrap_or("") {
                        "png" => "image/png",
                        "jpg" | "jpeg" => "image/jpeg",
                        "webp" => "image/webp",
                        "gif" => "image/gif",
                        "svg" => "image/svg+xml",
                        _ => continue,
                    };
                    return std::fs::read(&p).ok().map(|b| (b, ct));
                }
            }
        }
        None
    }
}

fn pct_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = vec![];
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 3 <= b.len() {
            if let Some(Ok(v)) = s.get(i + 1..i + 3).map(|h| u8::from_str_radix(h, 16)) {
                out.push(v);
                i += 3;
                continue;
            }
        }
        out.push(if b[i] == b'+' { b' ' } else { b[i] });
        i += 1;
    }
    String::from_utf8_lossy(&out).to_string()
}

/// Faixa do "senão" de \IfFileExists{..}{..}{senão} num corpo de macro.
fn else_range(body: &str) -> Option<(usize, usize)> {
    let ix = body.find("\\IfFileExists")?;
    let bp = latex::P::new(body);
    let mut j = ix + "\\IfFileExists".len();
    let mut parts = vec![];
    for _ in 0..3 {
        while body[j..].starts_with([' ', '\n', '\t', '\r']) || body[j..].starts_with('%') {
            if body[j..].starts_with('%') {
                j += body[j..].find('\n').map_or(body.len() - j, |x| x + 1);
            } else {
                j += 1;
            }
        }
        if !body[j..].starts_with('{') {
            return None;
        }
        let k = bp.match_brace(j, body.len())?;
        parts.push((j, k));
        j = k;
    }
    Some(parts[2])
}

/// Fonte .otf do cache do tectonic (a mesma que o PDF usa).
fn font_file(name: &str) -> Option<std::fs::File> {
    if !name.ends_with(".otf") || !name.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'-' || c == b'.') || name.contains("..") {
        return None;
    }
    let home = std::env::var("HOME").unwrap_or_default();
    let base = std::env::var("XDG_CACHE_HOME").map(PathBuf::from).unwrap_or_else(|_| Path::new(&home).join(".cache"));
    for t in ["tectonic", "Tectonic"] {
        let data = base.join(t).join("bundles").join("data");
        if let Ok(rd) = std::fs::read_dir(&data) {
            for e in rd.flatten() {
                if let Ok(f) = std::fs::File::open(e.path().join(name)) {
                    return Some(f);
                }
            }
        }
    }
    None
}

fn header(k: &str, v: &str) -> Header {
    Header::from_bytes(k.as_bytes(), v.as_bytes()).unwrap()
}

fn handle(app: &mut App, mut req: Request) {
    let url = req.url().to_string();
    let (path, qs) = url.split_once('?').unwrap_or((&url, ""));
    let query: HashMap<String, String> = qs
        .split('&')
        .filter(|x| !x.is_empty())
        .map(|kv| {
            let (k, v) = kv.split_once('=').unwrap_or((kv, ""));
            (pct_decode(k), pct_decode(v))
        })
        .collect();
    let text = |s: &str, ct: &str| Response::from_string(s).with_header(header("Content-Type", ct)).with_header(header("Cache-Control", "no-store"));
    let res = match (req.method(), path) {
        (Method::Get, "/") => req.respond(text(&INDEX.replace("__TOKEN__", &app.token), "text/html; charset=utf-8")),
        (Method::Get, "/app.js") => req.respond(text(APP_JS, "application/javascript; charset=utf-8")),
        (Method::Get, "/app.css") => req.respond(text(APP_CSS, "text/css; charset=utf-8")),
        (Method::Get, "/pages.js") => req.respond(text(PAGES_JS, "application/javascript; charset=utf-8")),
        (Method::Get, "/layout.js") => req.respond(text(LAYOUT_JS, "application/javascript; charset=utf-8")),
        (Method::Get, "/api/pdf") => {
            let ok = query.get("t").map_or(false, |t| *t == app.token);
            let main = query.get("main").cloned().unwrap_or_default();
            let seq: u64 = query.get("seq").and_then(|s| s.parse().ok()).unwrap_or(0);
            let file = app.builds.lock().unwrap().get(&main).and_then(|s| s.ok.clone()).filter(|r| r.seq == seq || seq == 0).map(|r| r.pdf.clone());
            match (ok, file.and_then(|p| std::fs::File::open(p).ok())) {
                (true, Some(f)) => req.respond(Response::from_file(f).with_header(header("Content-Type", "application/pdf")).with_header(header("Cache-Control", "private, max-age=3600"))),
                _ => req.respond(Response::from_string("").with_status_code(404)),
            }
        }
        (Method::Get, p) if p.starts_with("/font/") => match font_file(&p[6..]) {
            Some(f) => req.respond(Response::from_file(f).with_header(header("Content-Type", "font/otf")).with_header(header("Cache-Control", "max-age=86400"))),
            None => req.respond(Response::from_string("").with_status_code(404)),
        },
        (Method::Get, "/api/img") => match app.image(&query) {
            Some((bytes, ct)) => req.respond(Response::from_data(bytes).with_header(header("Content-Type", ct))),
            None => req.respond(Response::from_string("").with_status_code(404)),
        },
        (Method::Post, p) if p.starts_with("/api/") => {
            let ok = req.headers().iter().any(|h| h.field.equiv("x-token") && h.value.as_str() == app.token);
            let mut body = String::new();
            let _ = req.as_reader().read_to_string(&mut body);
            let out = if !ok {
                Err((403, "token inválido".to_string()))
            } else {
                let v: Value = serde_json::from_str(&body).unwrap_or(json!({}));
                app.api(&p[5..], v)
            };
            let (code, v) = match out {
                Ok(v) => (200, v),
                Err((c, m)) => (c, json!({"error": m})),
            };
            req.respond(text(&v.to_string(), "application/json; charset=utf-8").with_status_code(code))
        }
        _ => req.respond(Response::from_string("não encontrado").with_status_code(404)),
    };
    if let Err(e) = res {
        eprintln!("erro de resposta: {}", e);
    }
}

fn is_wsl() -> bool {
    std::fs::read_to_string("/proc/version").map_or(false, |v| v.to_lowercase().contains("microsoft"))
}

fn open_window(url: &str) {
    use std::process::{Command, Stdio};
    let app = format!("--app={}", url);
    let quiet = |c: &mut Command| {
        c.stdout(Stdio::null()).stderr(Stdio::null());
    };
    if cfg!(windows) {
        let mut c = Command::new("cmd");
        c.args(["/C", "start", "", "msedge", &app]);
        quiet(&mut c);
        if c.status().map_or(false, |s| s.success()) {
            return;
        }
        let mut c = Command::new("cmd");
        c.args(["/C", "start", "", url]);
        let _ = c.status();
    } else if is_wsl() {
        let mut c = Command::new("powershell.exe");
        c.current_dir("/mnt/c").args(["-NoProfile", "-Command", &format!("Start-Process msedge -ArgumentList '{}'", app)]);
        quiet(&mut c);
        if c.status().map_or(false, |s| s.success()) {
            return;
        }
        let mut c = Command::new("cmd.exe");
        c.current_dir("/mnt/c").args(["/C", "start", "", url]);
        quiet(&mut c);
        let _ = c.status();
    } else if cfg!(target_os = "macos") {
        let _ = Command::new("open").arg(url).status();
    } else {
        let mut c = Command::new("xdg-open");
        c.arg(url);
        quiet(&mut c);
        let _ = c.status();
    }
}

/// `tex-editor --check <pasta>`: confere em todos os .tex que o parser
/// reconstrói o fonte byte a byte e conta os trechos travados.
fn check(root: &Path) {
    let mut files = vec![];
    walk(root, root, &mut files);
    let (mut total, mut bad, mut locked, mut blocks) = (0, 0, 0, 0);
    for f in &files {
        let Ok(raw) = std::fs::read(root.join(f)) else { continue };
        let Ok((src, _, _)) = decode(raw) else {
            println!("NÃO-UTF8  {}", f);
            continue;
        };
        total += 1;
        let bl = parse(&src);
        blocks += bl.len();
        let cat: String = bl.iter().map(|b| b.src.as_str()).collect();
        if cat != src {
            bad += 1;
            println!("DIFERENTE {}", f);
        }
        for b in bl.iter().filter(|b| b.locked) {
            locked += 1;
            if locked <= 15 {
                println!("TRAVADO   {} :: {}", f, b.src.chars().take(110).collect::<String>().replace('\n', "⏎"));
            }
        }
    }
    println!("{} arquivos, {} blocos, {} reconstruções diferentes, {} blocos travados", total, blocks, bad, locked);
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().map(|a| a.as_str()) == Some("--check") {
        check(Path::new(args.get(1).map(|s| s.as_str()).unwrap_or(".")));
        return;
    }
    let no_open = args.iter().any(|a| a == "--no-open");
    let arg = args.iter().find(|a| !a.starts_with("--")).cloned().unwrap_or_else(|| ".".into());
    let mut path = PathBuf::from(&arg);
    if !path.exists() && arg.starts_with('/') && cfg!(windows) {
        path = PathBuf::from(format!("\\\\wsl.localhost\\Ubuntu{}", arg.replace('/', "\\")));
    }
    if !path.exists() {
        eprintln!("Caminho não encontrado: {}", arg);
        std::process::exit(1);
    }
    let path = if path.is_absolute() { path } else { std::env::current_dir().unwrap().join(path) };
    let (root, initial) = if path.is_file() {
        let root = path.parent().unwrap().to_path_buf();
        let rel = path.file_name().unwrap().to_string_lossy().to_string();
        (root, Some(rel))
    } else {
        (path, None)
    };
    let seed = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).unwrap().as_nanos() ^ ((std::process::id() as u128) << 64);
    let token = format!("{:032x}", seed.wrapping_mul(0x9E3779B97F4A7C15F39CC0605CEDC835));
    let port: u16 = std::env::var("TEX_EDITOR_PORT").ok().and_then(|p| p.parse().ok()).unwrap_or(7357);
    let server = Server::http(("127.0.0.1", port)).or_else(|_| Server::http("127.0.0.1:0")).expect("não consegui abrir a porta");
    let addr = server.server_addr().to_ip().unwrap();
    let url = format!("http://localhost:{}/", addr.port());
    println!("tex-editor");
    println!("  pasta: {}", root.display());
    println!("  abra:  {}", url);
    println!("  (Ctrl+C encerra; alterações não salvas são perdidas)");
    if !no_open {
        open_window(&url);
    }
    let mut app = App { root, docs: HashMap::new(), token, initial, builds: Builds::default(), main_choice: HashMap::new() };
    for req in server.incoming_requests() {
        handle(&mut app, req);
    }
}
