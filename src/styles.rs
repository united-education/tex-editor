//! Lê \definecolor e \newtcolorbox dos .tex vizinhos para pintar as caixas
//! do editor com as mesmas cores e títulos do PDF.

use crate::latex::{flat_text, parse_fragment, P};
use serde_json::{json, Map, Value};
use std::collections::HashMap;
use std::path::Path;

fn base_colors() -> HashMap<String, (f64, f64, f64)> {
    let mut m = HashMap::new();
    for (n, c) in [
        ("red", (1.0, 0.0, 0.0)),
        ("green", (0.0, 1.0, 0.0)),
        ("blue", (0.0, 0.0, 1.0)),
        ("black", (0.0, 0.0, 0.0)),
        ("white", (1.0, 1.0, 1.0)),
        ("gray", (0.5, 0.5, 0.5)),
        ("lightgray", (0.75, 0.75, 0.75)),
        ("darkgray", (0.25, 0.25, 0.25)),
        ("orange", (1.0, 0.5, 0.0)),
        ("yellow", (1.0, 1.0, 0.0)),
        ("cyan", (0.0, 1.0, 1.0)),
        ("magenta", (1.0, 0.0, 1.0)),
        ("brown", (0.75, 0.5, 0.25)),
        ("purple", (0.75, 0.0, 0.25)),
        ("teal", (0.0, 0.5, 0.5)),
        ("violet", (0.5, 0.0, 0.5)),
        ("olive", (0.5, 0.5, 0.0)),
        ("lime", (0.75, 1.0, 0.0)),
        ("pink", (1.0, 0.75, 0.75)),
    ] {
        m.insert(n.to_string(), c);
    }
    m
}

fn eval(expr: &str, colors: &HashMap<String, (f64, f64, f64)>) -> Option<(f64, f64, f64)> {
    let parts: Vec<&str> = expr.trim().split('!').map(|s| s.trim()).collect();
    let mut cur = *colors.get(parts[0])?;
    let mut k = 1;
    while k < parts.len() {
        let pct: f64 = parts[k].parse().ok()?;
        let other = if k + 1 < parts.len() { *colors.get(parts[k + 1])? } else { (1.0, 1.0, 1.0) };
        let t = pct / 100.0;
        cur = (cur.0 * t + other.0 * (1.0 - t), cur.1 * t + other.1 * (1.0 - t), cur.2 * t + other.2 * (1.0 - t));
        k += 2;
    }
    Some(cur)
}

/// "0.8pt", "2mm", "0pt" -> pt
fn length_pt(v: &str) -> Option<f64> {
    let v = strip_braces(v).trim();
    let num: String = v.chars().take_while(|c| c.is_ascii_digit() || *c == '.' || *c == '-').collect();
    let x: f64 = num.parse().ok()?;
    let unit = v[num.len()..].trim();
    let f = match unit {
        "pt" => 1.0,
        "mm" => 2.84528,
        "cm" => 28.4528,
        "bp" => 1.00375,
        "in" => 72.27,
        "em" => 10.95,
        "ex" => 4.71,
        "" if x == 0.0 => 1.0,
        _ => return None,
    };
    Some(x * f)
}

fn hex(c: (f64, f64, f64)) -> String {
    let f = |x: f64| (x.clamp(0.0, 1.0) * 255.0).round() as u8;
    format!("#{:02x}{:02x}{:02x}", f(c.0), f(c.1), f(c.2))
}

fn split_top(s: &str) -> Vec<String> {
    let mut out = vec![];
    let mut d = 0;
    let mut cur = String::new();
    for ch in s.chars() {
        match ch {
            '{' => d += 1,
            '}' => d -= 1,
            ',' if d == 0 => {
                out.push(std::mem::take(&mut cur));
                continue;
            }
            _ => {}
        }
        cur.push(ch);
    }
    out.push(cur);
    out
}

fn strip_braces(s: &str) -> &str {
    let t = s.trim();
    if t.starts_with('{') && t.ends_with('}') {
        &t[1..t.len() - 1]
    } else {
        t
    }
}

/// Lista de (nome, args) para cada ocorrência de \cmd no texto.
fn find_cmds<'a>(src: &'a str, cmd: &str) -> Vec<Vec<(bool, &'a str)>> {
    let p = P::new(src);
    let pat = format!("\\{}", cmd);
    let mut out = vec![];
    let mut from = 0;
    while let Some(k) = src[from..].find(&pat) {
        let a = from + k;
        let j = a + pat.len();
        from = j;
        if src[j..].starts_with(|c: char| c.is_ascii_alphabetic()) {
            continue;
        }
        // reaproveita o leitor de argumentos do parser
        let args = read_args_pub(&p, src, j);
        out.push(args);
    }
    out
}

fn read_args_pub<'a>(p: &P, src: &'a str, mut j: usize) -> Vec<(bool, &'a str)> {
    let b = src.as_bytes();
    let mut out = vec![];
    while j < b.len() && out.len() < 6 {
        while j < b.len() && matches!(b[j], b' ' | b'\n' | b'\r' | b'\t') && out.len() > 0 && b.get(j + 1).map_or(false, |c| *c == b'{') {
            j += 1;
        }
        if b.get(j) == Some(&b'{') {
            match p.match_brace(j, b.len()) {
                Some(k) => {
                    out.push((false, &src[j + 1..k - 1]));
                    j = k;
                }
                None => break,
            }
        } else if b.get(j) == Some(&b'[') {
            match src[j..].find(']') {
                Some(k) => {
                    out.push((true, &src[j + 1..j + k]));
                    j = j + k + 1;
                }
                None => break,
            }
        } else {
            break;
        }
    }
    out
}

pub fn collect(doc: &Path, root: &Path) -> Value {
    // .tex no diretório do documento e nos ancestrais até a raiz
    let mut dirs = vec![];
    let mut d = doc.parent().map(|p| p.to_path_buf());
    while let Some(x) = d {
        dirs.push(x.clone());
        if x == root || !x.starts_with(root) {
            break;
        }
        d = x.parent().map(|p| p.to_path_buf());
    }
    let mut text = String::new();
    for dir in dirs.iter().rev() {
        if let Ok(rd) = std::fs::read_dir(dir) {
            let mut files: Vec<_> = rd.flatten().map(|e| e.path()).filter(|p| p.extension().map_or(false, |e| e == "tex" || e == "sty")).collect();
            files.sort();
            for f in files {
                if let Ok(t) = std::fs::read_to_string(&f) {
                    if t.contains("\\newtcolorbox") || t.contains("\\definecolor") || t.contains("\\newenvironment") {
                        text.push_str(&t.replace("\r\n", "\n"));
                        text.push('\n');
                    }
                }
            }
        }
    }
    let mut colors = base_colors();
    for args in find_cmds(&text, "definecolor") {
        let br: Vec<&str> = args.iter().filter(|a| !a.0).map(|a| a.1).collect();
        if br.len() >= 3 {
            let v = br[2].trim();
            let c = match br[1].trim() {
                "HTML" => {
                    let h = u32::from_str_radix(v.trim_start_matches('#'), 16).ok();
                    h.map(|h| (((h >> 16) & 255) as f64 / 255.0, ((h >> 8) & 255) as f64 / 255.0, (h & 255) as f64 / 255.0))
                }
                "rgb" => {
                    let n: Vec<f64> = v.split(',').filter_map(|x| x.trim().parse().ok()).collect();
                    (n.len() == 3).then(|| (n[0], n[1], n[2]))
                }
                "RGB" => {
                    let n: Vec<f64> = v.split(',').filter_map(|x| x.trim().parse().ok()).collect();
                    (n.len() == 3).then(|| (n[0] / 255.0, n[1] / 255.0, n[2] / 255.0))
                }
                "gray" => v.parse().ok().map(|g: f64| (g, g, g)),
                _ => None,
            };
            if let Some(c) = c {
                colors.insert(br[0].trim().to_string(), c);
            }
        }
    }
    for args in find_cmds(&text, "colorlet") {
        let br: Vec<&str> = args.iter().filter(|a| !a.0).map(|a| a.1).collect();
        if br.len() >= 2 {
            if let Some(c) = eval(br[1], &colors) {
                colors.insert(br[0].trim().to_string(), c);
            }
        }
    }

    let mut out = Map::new();
    for args in find_cmds(&text, "newtcolorbox") {
        let br: Vec<&str> = args.iter().filter(|a| !a.0).map(|a| a.1).collect();
        if br.len() < 2 {
            continue;
        }
        let name = br[0].trim().to_string();
        let mut st = Map::new();
        let mut boxrule_zero = false;
        let mut leftrule = false;
        for kv in split_top(br[1]) {
            let mut it = kv.splitn(2, '=');
            let k = it.next().unwrap_or("").trim();
            let v = it.next().unwrap_or("").trim();
            match k {
                "colback" | "colframe" | "coltitle" | "colbacktitle" => {
                    if let Some(c) = eval(strip_braces(v), &colors) {
                        st.insert(k.to_string(), json!(hex(c)));
                    }
                }
                "title" => {
                    let t = strip_braces(v);
                    // \ifstrempty{#1}{}{ — #1} -> some com o argumento
                    let t = match t.find("\\ifstrempty") {
                        Some(a) => &t[..a],
                        None => t,
                    };
                    let plain = flat_text(&parse_fragment(t)).trim().to_string();
                    st.insert("title".into(), json!(plain));
                }
                "boxrule" => {
                    let num: String = v.chars().take_while(|c| c.is_ascii_digit() || *c == '.').collect();
                    boxrule_zero = num.parse::<f64>().map_or(false, |x| x == 0.0);
                    if let Some(pt) = length_pt(v) {
                        st.insert("boxrule".into(), json!(pt));
                    }
                }
                "leftrule" | "rightrule" | "toprule" | "bottomrule" | "arc" | "boxsep" | "left" | "right" | "top" | "bottom" | "toptitle" | "bottomtitle" | "before skip" | "after skip" => {
                    if k == "leftrule" {
                        leftrule = true;
                    }
                    if let Some(pt) = length_pt(v) {
                        st.insert(k.replace(' ', "_"), json!(pt));
                    }
                }
                "fonttitle" => {
                    for z in ["Large", "large", "small", "footnotesize"] {
                        if v.contains(&format!("\\{}", z)) {
                            st.insert("titlesize".into(), json!(z));
                            break;
                        }
                    }
                }
                "fontupper" => {
                    if v.contains("\\itshape") {
                        st.insert("italic".into(), json!(true));
                    }
                    if v.contains("\\small") {
                        st.insert("small".into(), json!(true));
                    }
                }
                _ => {}
            }
        }
        st.insert("tcb".into(), json!(true));
        if boxrule_zero && leftrule {
            st.insert("bar".into(), json!(true));
        }
        out.insert(name, Value::Object(st));
    }
    // \newenvironment{resolvido}[1][]{...\begin{resolvidobox}...}{...}
    for args in find_cmds(&text, "newenvironment") {
        let br: Vec<&str> = args.iter().filter(|a| !a.0).map(|a| a.1).collect();
        if br.len() >= 2 {
            let name = br[0].trim();
            if let Some(a) = br[1].find("\\begin{") {
                let r = &br[1][a + 7..];
                if let Some(e) = r.find('}') {
                    let inner = &r[..e];
                    if let Some(st) = out.get(inner).cloned() {
                        out.entry(name.to_string()).or_insert(st);
                    }
                }
            }
        }
    }
    // \titleformat{\section}{\normalfont\Large\bfseries\color{verdebio}}... -> "#section"
    let tp = P::new(&text);
    let mut from = 0;
    while let Some(k) = text[from..].find("\\titleformat{") {
        let a = from + k + "\\titleformat".len();
        from = a;
        // argumentos {nome}[forma]{formato}, com quebras de linha entre eles
        let mut br: Vec<&str> = vec![];
        let mut j = a;
        while br.len() < 2 && j < text.len() {
            let rest = &text[j..];
            let t = rest.len() - rest.trim_start().len();
            j += t;
            if text[j..].starts_with('[') {
                match text[j..].find(']') {
                    Some(e) => { j += e + 1; continue; }
                    None => break,
                }
            }
            if !text[j..].starts_with('{') {
                break;
            }
            let Some(e) = tp.match_brace(j, text.len()) else { break };
            br.push(&text[j + 1..e - 1]);
            j = e;
        }
        if br.len() < 2 {
            continue;
        }
        let name = br[0].trim().trim_start_matches('\\');
        let f = br[1];
        let mut st = serde_json::Map::new();
        for z in ["Huge", "huge", "LARGE", "Large", "large", "normalsize", "small"] {
            if f.contains(&format!("\\{}", z)) {
                st.insert("size".into(), json!(z));
                break;
            }
        }
        st.insert("bold".into(), json!(f.contains("\\bfseries")));
        if let Some(k) = f.find("\\color{") {
            if let Some(e) = f[k + 7..].find('}') {
                if let Some(c) = eval(&f[k + 7..k + 7 + e], &colors) {
                    st.insert("color".into(), json!(hex(c)));
                }
            }
        }
        out.insert(format!("#{}", name), Value::Object(st));
    }
    // \newcommand{\estrategia}[1]{{\par\noindent\small\colorbox{verdebioclaro}{\bfseries\color{verdebio} Estratégia}\enspace\itshape #1\par}}
    // vira {"block": true, "small": true, "italic": true, "chip": "Estratégia", "chipbg": ..., "chipfg": ...}
    for args in find_cmds(&text, "newcommand") {
        let br: Vec<&str> = args.iter().filter(|a| !a.0).map(|a| a.1).collect();
        if br.len() < 2 || br[1].contains("\\includegraphics") || br[1].contains("\\begin{") {
            continue;
        }
        let name = br[0].trim();
        if !name.starts_with('\\') {
            continue;
        }
        let body = br[1].trim();
        let inner = strip_braces(body).trim();
        let mut st = serde_json::Map::new();
        let block = inner.starts_with("\\par") || inner.starts_with("\\vspace") && inner.contains("\\par");
        if !block {
            continue; // só os que abrem parágrafo próprio mudam o desenho
        }
        st.insert("block".into(), json!(true));
        let before_arg = inner.split('#').next().unwrap_or(inner);
        // \vspace{4pt} antes / depois do texto
        let after_arg = inner.rsplit('#').next().unwrap_or("");
        for (part, key) in [(before_arg, "vbefore"), (after_arg, "vafter")] {
            if inner.contains('#') || key == "vafter" {
                if let Some(k) = part.find("\\vspace{") {
                    if let Some(e) = part[k + 8..].find('}') {
                        if let Some(pt) = length_pt(&part[k + 8..k + 8 + e]) {
                            st.insert(key.into(), json!(pt));
                        }
                    }
                }
            }
        }
        if inner.contains("\\small") {
            st.insert("small".into(), json!(true));
        }
        if inner.contains("\\footnotesize") {
            st.insert("footnote".into(), json!(true));
        }
        if inner.contains("\\itshape") || inner.contains("\\em ") {
            st.insert("italic".into(), json!(true));
        }
        if let Some(k) = inner.find("\\color{") {
            if let Some(e) = inner[k + 7..].find('}') {
                // cor do texto fora da etiqueta
                let after_chip = inner.find("\\colorbox").map_or(true, |c| k < c || k > c + 40);
                if after_chip {
                    if let Some(c) = eval(&inner[k + 7..k + 7 + e], &colors) {
                        st.insert("color".into(), json!(hex(c)));
                    }
                }
            }
        }
        if let Some(cb) = before_arg.find("\\colorbox") {
            let cargs = find_cmds(&before_arg[cb..], "colorbox");
            if let Some(ca) = cargs.first() {
                let parts: Vec<&str> = ca.iter().filter(|a| !a.0).map(|a| a.1).collect();
                if parts.len() >= 2 {
                    if let Some(c) = eval(parts[0].trim(), &colors) {
                        st.insert("chipbg".into(), json!(hex(c)));
                    }
                    let label = parts[1];
                    if let Some(k) = label.find("\\color{") {
                        if let Some(e) = label[k + 7..].find('}') {
                            if let Some(c) = eval(&label[k + 7..k + 7 + e], &colors) {
                                st.insert("chipfg".into(), json!(hex(c)));
                            }
                        }
                    }
                    // texto da etiqueta sem comandos
                    let mut t = String::new();
                    let mut skip_arg = false;
                    let mut chars = label.chars().peekable();
                    while let Some(ch) = chars.next() {
                        if ch == '\\' {
                            let mut w = String::new();
                            while let Some(&n) = chars.peek() {
                                if n.is_ascii_alphabetic() { w.push(n); chars.next(); } else { break; }
                            }
                            skip_arg = w == "color" || w == "textcolor";
                            if w.is_empty() { chars.next(); }
                            continue;
                        }
                        if ch == '{' && skip_arg {
                            for n in chars.by_ref() { if n == '}' { break; } }
                            skip_arg = false;
                            continue;
                        }
                        if ch != '{' && ch != '}' {
                            t.push(ch);
                        }
                    }
                    let t = t.split_whitespace().collect::<Vec<_>>().join(" ");
                    if !t.is_empty() {
                        st.insert("chip".into(), json!(t));
                    }
                }
            }
        } else if !inner.contains('#') {
            // \resolucao sem argumento e sem etiqueta: nada a desenhar além do parágrafo
        }
        out.insert(name.to_string(), Value::Object(st));
    }
    Value::Object(out)
}
