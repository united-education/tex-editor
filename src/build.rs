//! Compilação em segundo plano (tectonic + SyncTeX) e mapa
//! linha do fonte -> retângulos nas páginas do PDF.

use std::collections::HashMap;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, Mutex};
use std::time::Instant;

/// Origem de cada linha do arquivo achatado: (índice do arquivo, linha 1-based).
#[derive(Default, Clone)]
pub struct LineMap {
    pub files: Vec<String>,
    pub lines: Vec<(u16, u32)>,
}

pub const NO_FILE: u16 = u16::MAX;

/// Primeira linha do arquivo compilado: escreve no .log as medidas da página
/// (em pt), já com o geometry aplicado.
pub const GEOM_PROBE: &str = "\\makeatletter\\ifdefined\\AddToHook\\AddToHook{begindocument/end}{\\typeout{TEXEDG \\strip@pt\\textwidth\\space\\strip@pt\\textheight\\space\\f@size\\space\\strip@pt\\dimexpr\\baselineskip\\relax\\space\\strip@pt\\parindent\\space\\strip@pt\\dimexpr\\parskip\\relax}\\typeout{TEXEDM \\strip@pt\\dimexpr1in+\\voffset+\\topmargin+\\headheight+\\headsep\\relax\\space\\strip@pt\\dimexpr1in+\\hoffset+\\oddsidemargin\\relax\\space\\strip@pt\\dimexpr1in+\\hoffset+\\evensidemargin\\relax\\space\\strip@pt\\paperwidth\\space\\strip@pt\\paperheight}}\\fi\\makeatother\n";

/// Medidas da página, em pt.
#[derive(Clone, Copy, Default, serde::Serialize)]
pub struct Geom {
    pub tw: f32,
    pub th: f32,
    pub fs: f32,
    pub bs: f32,
    pub pi: f32,
    pub ps: f32,
    pub top: f32,
    pub odd: f32,
    pub even: f32,
    pub pw: f32,
    pub ph: f32,
}

fn geom_of(log: &str) -> Option<Geom> {
    let nums = |tag: &str| -> Vec<f32> {
        log.lines().find_map(|l| l.strip_prefix(tag)).map(|r| r.split_whitespace().filter_map(|x| x.parse().ok()).collect()).unwrap_or_default()
    };
    let a = nums("TEXEDG ");
    let b = nums("TEXEDM ");
    (a.len() == 6 && b.len() == 5).then(|| Geom { tw: a[0], th: a[1], fs: a[2], bs: a[3], pi: a[4], ps: a[5], top: b[0], odd: b[1], even: b[2], pw: b[3], ph: b[4] })
}

impl LineMap {
    pub fn file_idx(&mut self, rel: &str) -> u16 {
        if let Some(k) = self.files.iter().position(|f| f == rel) {
            return k as u16;
        }
        self.files.push(rel.to_string());
        (self.files.len() - 1) as u16
    }
    /// Acrescenta `text` (vindo de `file`, a partir da linha `line`) à saída.
    pub fn emit(&mut self, out: &mut String, text: &str, file: u16, mut line: u32) {
        for ch in text.chars() {
            if out.is_empty() || out.ends_with('\n') {
                self.lines.push((file, line));
            }
            out.push(ch);
            if ch == '\n' {
                line += 1;
            }
        }
    }
}

#[derive(Clone, Copy, serde::Serialize)]
pub struct Rect {
    pub p: u16,
    pub x0: f32,
    pub y0: f32,
    pub x1: f32,
    pub y1: f32,
}

pub struct Job {
    pub main: String,
    pub main_dir: PathBuf,
    pub text: String,
    pub map: LineMap,
    pub snapshots: HashMap<String, String>,
}

pub struct BuildResult {
    pub seq: u64,
    pub main: String,
    pub pdf: PathBuf,
    pub pages: usize,
    pub page_w: f32,
    pub page_h: f32,
    /// rel -> linha -> retângulos
    pub hits: HashMap<String, HashMap<u32, Vec<Rect>>>,
    /// rel -> linha -> linhas de base impressas (página, y em meio bp)
    pub bases: Bases,
    pub snapshots: HashMap<String, String>,
    pub ms: u128,
    pub geom: Option<Geom>,
    /// \label -> número ("fig:cap03-gota" -> "1.1"), do .aux
    pub labels: HashMap<String, String>,
}

fn aux_labels(aux: &str) -> HashMap<String, String> {
    let mut out = HashMap::new();
    for l in aux.lines() {
        let Some(r) = l.strip_prefix("\\newlabel{") else { continue };
        let Some(e) = r.find("}{{") else { continue };
        let name = &r[..e];
        let rest = &r[e + 3..];
        let Some(n) = rest.find('}') else { continue };
        out.insert(name.to_string(), rest[..n].to_string());
    }
    out
}

#[derive(Clone, serde::Serialize)]
pub struct BuildError {
    pub rel: Option<String>,
    pub line: Option<u32>,
    pub msg: String,
}

#[derive(Default)]
pub struct Slot {
    pub running: bool,
    pub pending: Option<Job>,
    pub last_text: String,
    pub seq: u64,
    pub ok: Option<Arc<BuildResult>>,
    pub errors: Vec<BuildError>,
    pub failed_seq: u64,
    pub log: String,
}

pub type Bases = HashMap<String, HashMap<u32, Vec<(u16, i32)>>>;

pub type Builds = Arc<Mutex<HashMap<String, Slot>>>;

pub fn tectonic() -> Option<PathBuf> {
    if let Ok(p) = std::env::var("TEX_EDITOR_TECTONIC") {
        return Some(PathBuf::from(p));
    }
    let exe = if cfg!(windows) { "tectonic.exe" } else { "tectonic" };
    if let Ok(path) = std::env::var("PATH") {
        let sep = if cfg!(windows) { ';' } else { ':' };
        for d in path.split(sep) {
            let p = Path::new(d).join(exe);
            if p.is_file() {
                return Some(p);
            }
        }
    }
    let home = std::env::var("HOME").unwrap_or_default();
    let p = Path::new(&home).join(".local/bin").join(exe);
    p.is_file().then_some(p)
}

fn build_dir(main: &str) -> PathBuf {
    let safe: String = main.chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '_' }).collect();
    std::env::temp_dir().join("tex-editor-build").join(safe)
}

/// Coloca o trabalho na fila do arquivo principal; dispara se estiver livre.
pub fn submit(builds: &Builds, job: Job) {
    let mut g = builds.lock().unwrap();
    let slot = g.entry(job.main.clone()).or_default();
    if slot.last_text == job.text && slot.pending.is_none() {
        return;
    }
    if slot.running {
        slot.pending = Some(job);
        return;
    }
    slot.running = true;
    slot.last_text = job.text.clone();
    slot.seq += 1;
    let seq = slot.seq;
    drop(g);
    spawn(builds.clone(), job, seq);
}

fn spawn(builds: Builds, job: Job, seq: u64) {
    std::thread::spawn(move || {
        let main = job.main.clone();
        let res = run(&job, seq);
        let mut g = builds.lock().unwrap();
        let slot = g.entry(main.clone()).or_default();
        match res {
            Ok((r, log)) => {
                slot.ok = Some(Arc::new(r));
                slot.errors.clear();
                slot.log = log;
            }
            Err((errs, log)) => {
                slot.errors = errs;
                slot.failed_seq = seq;
                slot.log = log;
            }
        }
        if let Some(next) = slot.pending.take() {
            if next.text != slot.last_text {
                slot.last_text = next.text.clone();
                slot.seq += 1;
                let s2 = slot.seq;
                drop(g);
                spawn(builds, next, s2);
                return;
            }
        }
        slot.running = false;
    });
}

fn run(job: &Job, seq: u64) -> Result<(BuildResult, String), (Vec<BuildError>, String)> {
    let t0 = Instant::now();
    let Some(tec) = tectonic() else {
        return Err((vec![BuildError { rel: None, line: None, msg: "O tectonic não foi encontrado. Instale com: curl --proto '=https' --tlsv1.2 -fsSL https://drop-sh.fullyjustified.net | sh  (e mova para ~/.local/bin)".into() }], String::new()));
    };
    let dir = build_dir(&job.main);
    let _ = std::fs::create_dir_all(&dir);
    let tex = dir.join("main.tex");
    if let Err(e) = std::fs::write(&tex, &job.text) {
        return Err((vec![BuildError { rel: None, line: None, msg: e.to_string() }], String::new()));
    }
    let out = Command::new(&tec)
        .current_dir(&dir)
        .args(["-X", "compile", "--synctex", "--keep-intermediates", "--keep-logs", "-Z"])
        .arg(format!("search-path={}", job.main_dir.display()))
        .arg("--outdir")
        .arg(&dir)
        .arg("main.tex")
        .output();
    let out = match out {
        Ok(o) => o,
        Err(e) => return Err((vec![BuildError { rel: None, line: None, msg: format!("Não consegui rodar o tectonic: {}", e) }], String::new())),
    };
    let mut log = String::from_utf8_lossy(&out.stdout).to_string();
    log.push_str(&String::from_utf8_lossy(&out.stderr));
    if !out.status.success() {
        return Err((parse_errors(&log, &dir, &job.map), tail(&log)));
    }
    let pdf_src = dir.join("main.pdf");
    let pdf = dir.join(format!("out-{}.pdf", seq));
    // mantém só o PDF atual e o anterior (o navegador pode estar lendo)
    if let Ok(rd) = std::fs::read_dir(&dir) {
        for e in rd.flatten() {
            let n = e.file_name().to_string_lossy().to_string();
            if let Some(k) = n.strip_prefix("out-").and_then(|r| r.strip_suffix(".pdf")).and_then(|r| r.parse::<u64>().ok()) {
                if k + 1 < seq {
                    let _ = std::fs::remove_file(e.path());
                }
            }
        }
    }
    if std::fs::copy(&pdf_src, &pdf).is_err() {
        return Err((vec![BuildError { rel: None, line: None, msg: "A compilação não gerou PDF.".into() }], tail(&log)));
    }
    let geom = geom_of(&std::fs::read_to_string(dir.join("main.log")).unwrap_or_default()).or_else(|| geom_of(&log));
    // o tamanho do papel vem do LaTeX (o /MediaBox costuma estar comprimido)
    let (page_w, page_h) = match geom {
        Some(g) if g.pw > 0.0 => (g.pw * 72.0 / 72.27, g.ph * 72.0 / 72.27),
        _ => media_box(&pdf).unwrap_or((595.0, 842.0)),
    };
    // cabeçalho e rodapé ficam de fora: só a mancha de texto conta
    let cuts = match geom {
        Some(g) => ((g.top - 2.0) * 72.0 / 72.27, (g.top + g.th) * 72.0 / 72.27 + 8.0),
        None => (page_h * 0.075, page_h * 0.925),
    };
    let (pages, hits, bases) = synctex(&dir.join("main.synctex.gz"), &job.map, cuts).unwrap_or_default();
    Ok((
        BuildResult { seq, main: job.main.clone(), pdf, pages, page_w, page_h, hits, bases, snapshots: job.snapshots.clone(), ms: t0.elapsed().as_millis(), geom, labels: aux_labels(&std::fs::read_to_string(dir.join("main.aux")).unwrap_or_default()) },
        tail(&log),
    ))
}

fn tail(s: &str) -> String {
    let lines: Vec<&str> = s.lines().filter(|l| !l.starts_with("note: download")).collect();
    lines[lines.len().saturating_sub(40)..].join("\n")
}

fn parse_errors(log: &str, dir: &Path, map: &LineMap) -> Vec<BuildError> {
    // o .log do TeX tem "! Mensagem" seguido de "l.123 ..."
    let mut out = vec![];
    let texlog = std::fs::read_to_string(dir.join("main.log")).unwrap_or_default();
    let lines: Vec<&str> = texlog.lines().collect();
    let mut i = 0;
    while i < lines.len() && out.len() < 20 {
        if let Some(msg) = lines[i].strip_prefix("! ") {
            let mut line = None;
            for l in lines.iter().skip(i + 1).take(12) {
                if let Some(r) = l.strip_prefix("l.") {
                    line = r.split(|c: char| !c.is_ascii_digit()).next().and_then(|n| n.parse::<usize>().ok());
                    break;
                }
            }
            let (rel, sl) = match line.and_then(|l| map.lines.get(l.saturating_sub(1))) {
                Some(&(f, l)) if f != NO_FILE => (map.files.get(f as usize).cloned(), Some(l)),
                _ => (None, None),
            };
            out.push(BuildError { rel, line: sl, msg: msg.to_string() });
        }
        i += 1;
    }
    if out.is_empty() {
        for l in log.lines().filter(|l| l.starts_with("error")) {
            out.push(BuildError { rel: None, line: None, msg: l.to_string() });
        }
    }
    if out.is_empty() {
        out.push(BuildError { rel: None, line: None, msg: "A compilação falhou (veja o registro).".into() });
    }
    out
}

fn media_box(pdf: &Path) -> Option<(f32, f32)> {
    let mut f = std::fs::File::open(pdf).ok()?;
    let mut buf = vec![0u8; 400_000];
    let n = f.read(&mut buf).ok()?;
    let s = String::from_utf8_lossy(&buf[..n]);
    let k = s.find("/MediaBox")?;
    let r = &s[k + 9..];
    let a = r.find('[')? + 1;
    let b = r.find(']')?;
    let v: Vec<f32> = r[a..b].split_whitespace().filter_map(|x| x.parse().ok()).collect();
    (v.len() == 4).then(|| (v[2] - v[0], v[3] - v[1]))
}

const SP_TO_BP: f32 = 72.0 / 72.27 / 65536.0;

/// Lê o .synctex.gz e devolve (nº de páginas, rel -> linha -> retângulos).
fn synctex(path: &Path, map: &LineMap, cuts: (f32, f32)) -> Option<(usize, HashMap<String, HashMap<u32, Vec<Rect>>>, Bases)> {
    let f = std::fs::File::open(path).ok()?;
    let mut s = String::new();
    flate2::read::GzDecoder::new(f).read_to_string(&mut s).ok()?;
    let mut main_tag = None;
    let mut unit = 1.0f32;
    let mut out: HashMap<String, HashMap<u32, Vec<Rect>>> = HashMap::new();
    let mut bases: Bases = HashMap::new();
    let mut page = 0u16;
    let mut pages = 0usize;
    let (top_cut, bot_cut) = cuts;
    for l in s.lines() {
        if let Some(r) = l.strip_prefix("Input:") {
            if let Some((tag, p)) = r.split_once(':') {
                if p.ends_with("main.tex") && main_tag.is_none() {
                    main_tag = tag.parse::<u32>().ok();
                }
            }
            continue;
        }
        if let Some(u) = l.strip_prefix("Unit:") {
            unit = u.trim().parse().unwrap_or(1.0);
            continue;
        }
        let b = l.as_bytes();
        if b.is_empty() {
            continue;
        }
        if b[0] == b'{' {
            page = l[1..].parse().unwrap_or(0);
            pages = pages.max(page as usize);
            continue;
        }
        let t = b[0];
        if !matches!(t, b'(' | b'g' | b'k' | b'x' | b'$' | b'h') {
            continue;
        }
        let mut parts = l[1..].split(':');
        let (Some(a), Some(p)) = (parts.next(), parts.next()) else { continue };
        let mut tl = a.split(',');
        let tag: u32 = tl.next().and_then(|x| x.parse().ok()).unwrap_or(0);
        let line: u32 = tl.next().and_then(|x| x.parse().ok()).unwrap_or(0);
        if Some(tag) != main_tag || line == 0 {
            continue;
        }
        let mut xy = p.split(',');
        let x: f32 = xy.next().and_then(|v| v.parse().ok()).unwrap_or(0.0) * unit * SP_TO_BP;
        let y: f32 = xy.next().and_then(|v| v.parse().ok()).unwrap_or(0.0) * unit * SP_TO_BP;
        let (mut x1, mut y0, mut y1) = (x + 2.0, y - 8.0, y + 2.5);
        if t == b'(' || t == b'h' {
            let whd: Vec<f32> = parts.next().unwrap_or("").split(',').filter_map(|v| v.parse().ok()).collect();
            if whd.len() == 3 {
                let (w, h, d) = (whd[0] * unit * SP_TO_BP, whd[1] * unit * SP_TO_BP, whd[2] * unit * SP_TO_BP);
                if w <= 0.5 || h + d > 40.0 {
                    continue;
                }
                x1 = x + w;
                y0 = y - h;
                y1 = y + d;
            }
        }
        if y < top_cut || y > bot_cut {
            continue;
        }
        let Some(&(f, sl)) = map.lines.get(line as usize - 1) else { continue };
        if f == NO_FILE {
            continue;
        }
        let rel = &map.files[f as usize];
        // cola, kern e matemática ficam na linha de base: uma por linha impressa
        if matches!(t, b'g' | b'k' | b'x' | b'$') {
            let key = (page, (y * 2.0).round() as i32);
            let bv = bases.entry(rel.clone()).or_default().entry(sl).or_default();
            if !bv.contains(&key) {
                bv.push(key);
            }
        }
        let v = out.entry(rel.clone()).or_default().entry(sl).or_default();
        // junta com o último retângulo da mesma linha/página quando se tocam
        if let Some(last) = v.last_mut() {
            if last.p == page && y0 < last.y1 + 3.0 && y1 > last.y0 - 3.0 {
                last.x0 = last.x0.min(x);
                last.x1 = last.x1.max(x1);
                last.y0 = last.y0.min(y0);
                last.y1 = last.y1.max(y1);
                continue;
            }
        }
        v.push(Rect { p: page, x0: x, y0, x1, y1 });
    }
    Some((pages, out, bases))
}

/// Retângulos (um por página) de cada faixa de linhas [l0, l1].
pub fn rects_for(hits: &HashMap<u32, Vec<Rect>>, l0: u32, l1: u32) -> Vec<Rect> {
    let mut per: Vec<Rect> = vec![];
    for l in l0..=l1 {
        if let Some(v) = hits.get(&l) {
            for r in v {
                match per.iter_mut().find(|x| x.p == r.p) {
                    Some(x) => {
                        x.x0 = x.x0.min(r.x0);
                        x.y0 = x.y0.min(r.y0);
                        x.x1 = x.x1.max(r.x1);
                        x.y1 = x.y1.max(r.y1);
                    }
                    None => per.push(*r),
                }
            }
        }
    }
    per.sort_by_key(|r| r.p);
    per
}
