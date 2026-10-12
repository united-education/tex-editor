//! Parser LaTeX "preservador de fonte".
//!
//! O documento é dividido em BLOCOS que cobrem o arquivo inteiro, sem
//! lacunas: concat(blocos.src) == fonte. Blocos de texto (parágrafo, item,
//! título) têm um modelo inline editável; todo o resto é mantido como fonte
//! bruta. Cada caractere exibido guarda o trecho de fonte que o gerou
//! (`segs`), então um bloco não editado é reescrito byte a byte, e um bloco
//! editado só muda nos caracteres que de fato mudaram.

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "t", rename_all = "lowercase")]
pub enum Inline {
    Text {
        v: String,
        #[serde(skip, default)]
        segs: Vec<String>,
    },
    Fmt {
        cmd: String,
        c: Vec<Inline>,
    },
    Group {
        c: Vec<Inline>,
    },
    Macro {
        name: String,
        args: Vec<Arg>,
    },
    Atom {
        src: String,
        #[serde(default)]
        kind: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        d: Option<String>,
    },
    Par,
    Caret,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Arg {
    pub opt: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub raw: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub c: Option<Vec<Inline>>,
}

#[derive(Clone, Debug, Serialize)]
pub struct Cell {
    pub span: usize,
    pub inl: Vec<Inline>,
}

#[derive(Clone, Debug, Serialize, Default)]
pub struct Block {
    pub id: usize,
    pub kind: &'static str,
    pub start: usize,
    pub end: usize,
    #[serde(skip)]
    pub pre: String,
    #[serde(skip)]
    pub post: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub inl: Vec<Inline>,
    pub depth: usize,
    pub level: u8,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub env: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub opt: Option<String>,
    pub src: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub table: Option<Vec<Vec<Cell>>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target: Option<String>,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub locked: bool,
}

impl Block {
    pub fn is_text(&self) -> bool {
        matches!(self.kind, "para" | "item" | "heading")
    }
    pub fn content_range(&self) -> (usize, usize) {
        (self.start + self.pre.len(), self.end - self.post.len())
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Mode {
    Para,
    Item,
    Inner,
}

const FMT: &[&str] = &[
    "textbf", "textit", "emph", "underline", "textsuperscript", "textsubscript", "texttt",
    "textsc", "textsf", "textup", "textsl", "textrm", "textmd", "textnormal", "uline",
];

const RAW_ENVS: &[&str] = &[
    "tabular", "tabular*", "tabularx", "longtable", "table", "table*", "figure", "figure*",
    "wrapfigure", "tikzpicture", "verbatim", "Verbatim", "lstlisting", "minted", "comment",
    "filecontents", "picture", "circuitikz", "forest", "chemfig",
];

const MATH_ENVS: &[&str] = &[
    "equation", "equation*", "align", "align*", "gather", "gather*", "multline", "multline*",
    "eqnarray", "eqnarray*", "displaymath", "alignat", "alignat*", "flalign", "flalign*",
];

pub const LIST_ENVS: &[&str] = &["itemize", "enumerate", "description", "compactitem", "compactenum"];

const BRACE_ARG_ENVS: &[&str] = &["minipage", "multicols", "multicols*", "adjustwidth"];

const HEADINGS: &[(&str, u8)] = &[
    ("part", 0),
    ("chapter", 1),
    ("section", 2),
    ("subsection", 3),
    ("subsubsection", 4),
    ("paragraph", 5),
    ("subparagraph", 6),
];

const STRUCT_CMDS: &[&str] = &[
    "documentclass", "usepackage", "RequirePackage", "input", "include", "includeonly",
    "newcommand", "renewcommand", "providecommand", "newenvironment", "renewenvironment",
    "definecolor", "colorlet", "newtcolorbox", "tcbset", "tcbuselibrary", "setlength",
    "setcounter", "addtocounter", "maketitle", "tableofcontents", "listoffigures",
    "listoftables", "appendix", "frontmatter", "mainmatter", "backmatter", "clearpage",
    "cleardoublepage", "newpage", "pagestyle", "thispagestyle", "title", "author", "date",
    "bibliography", "bibliographystyle", "printbibliography", "addbibresource", "geometry",
    "hypersetup", "makeatletter", "makeatother", "newcounter", "numberwithin", "captionsetup",
    "setlist", "newlist", "pagebreak", "vfill", "graphicspath", "fancyhf", "fancyhead",
    "fancyfoot", "titleformat", "titlespacing", "DeclareMathOperator", "newtheorem",
    "theoremstyle", "pagenumbering", "selectlanguage", "AtBeginDocument", "AtEndDocument",
    "def", "let", "newif", "linespread", "onehalfspacing", "doublespacing", "printindex",
    "makeindex", "sisetup", "NewDocumentCommand", "RenewDocumentCommand", "DeclareRobustCommand",
    "newlength", "setstretch", "renewenvironment", "usetikzlibrary", "pgfplotsset",
];

const LAYOUT: &[&str] = &[
    "noindent", "indent", "medskip", "bigskip", "smallskip", "par", "newline", "linebreak",
    "centering", "raggedright", "raggedleft", "small", "footnotesize", "scriptsize", "tiny",
    "large", "Large", "LARGE", "huge", "Huge", "normalsize", "bfseries", "itshape", "mdseries",
    "upshape", "rmfamily", "sffamily", "ttfamily", "scshape", "normalfont", "color", "vspace",
    "hspace", "hfill", "label", "nopagebreak", "allowbreak", "nobreak", "protect", "relax",
    "strut", "null", "leavevmode", "selectfont", "fontsize", "addvspace", "needspace",
    "enlargethispage", "index", "hyphenation", "phantomsection", "vskip", "hskip", "unskip",
    "ignorespaces", "rowcolor", "arraystretch", "noalign", "FloatBarrier", "columnbreak",
];

const REFS: &[&str] = &[
    "ref", "eqref", "pageref", "cref", "Cref", "autoref", "nameref", "cite", "citep", "citet",
    "footnotemark", "hyperref", "url",
];

const NO_TEXT_ARGS: &[&str] = &[
    "url", "href", "includegraphics", "input", "include", "begin", "end", "color", "colorbox",
    "definecolor", "newcommand", "renewcommand", "setlength", "hspace", "vspace", "rule",
    "multicolumn", "cline", "usepackage", "documentclass", "label", "ref",
];

fn sym(name: &str) -> Option<&'static str> {
    Some(match name {
        "ldots" | "dots" | "textellipsis" => "…",
        "quad" => "\u{2003}",
        "qquad" => "\u{2003}\u{2003}",
        "textdegree" => "°",
        "S" => "§",
        "P" => "¶",
        "textperiodcentered" => "·",
        "textbullet" => "•",
        "copyright" | "textcopyright" => "©",
        "textregistered" => "®",
        "texttrademark" => "™",
        "textasciitilde" => "~",
        "textbackslash" => "\\",
        "textasciicircum" => "^",
        "textunderscore" => "_",
        "textbar" => "|",
        "textless" => "<",
        "textgreater" => ">",
        "LaTeX" => "LaTeX",
        "TeX" => "TeX",
        "ss" => "ß",
        "o" => "ø",
        "O" => "Ø",
        "ae" => "æ",
        "oe" => "œ",
        "aa" => "å",
        "l" => "ł",
        "i" => "ı",
        "textquotedblleft" => "“",
        "textquotedblright" => "”",
        "textquoteleft" => "‘",
        "textquoteright" => "’",
        "textendash" => "–",
        "textemdash" => "—",
        "guillemotleft" | "guillemetleft" => "«",
        "guillemotright" | "guillemetright" => "»",
        "euro" | "texteuro" => "€",
        "textmu" => "µ",
        "checkmark" => "✓",
        "dag" | "textdagger" => "†",
        "ddag" | "textdaggerdbl" => "‡",
        "textordmasculine" => "º",
        "textordfeminine" => "ª",
        "textonehalf" => "½",
        "texttimes" => "×",
        "textminus" => "−",
        _ => return None,
    })
}

fn combining(c: u8) -> Option<char> {
    Some(match c {
        b'\'' => '\u{0301}',
        b'`' => '\u{0300}',
        b'^' => '\u{0302}',
        b'"' => '\u{0308}',
        b'~' => '\u{0303}',
        b'=' => '\u{0304}',
        b'.' => '\u{0307}',
        _ => return None,
    })
}

// ------------------------------------------------------------------ parser

#[derive(Default)]
struct Tb {
    v: String,
    segs: Vec<String>,
}
impl Tb {
    fn push(&mut self, d: char, src: &str) {
        self.v.push(d);
        self.segs.push(src.to_string());
    }
    fn flush(&mut self, out: &mut Vec<Inline>) {
        if !self.v.is_empty() {
            out.push(Inline::Text { v: std::mem::take(&mut self.v), segs: std::mem::take(&mut self.segs) });
        }
    }
}

fn atom(src: &str, kind: &str, d: Option<&str>) -> Inline {
    Inline::Atom { src: src.to_string(), kind: kind.to_string(), d: d.map(|x| x.to_string()) }
}

pub struct P<'a> {
    s: &'a str,
    b: &'a [u8],
}

impl<'a> P<'a> {
    pub fn new(s: &'a str) -> Self {
        P { s, b: s.as_bytes() }
    }

    fn starts(&self, i: usize, pat: &str) -> bool {
        self.s.get(i..).map_or(false, |r| r.starts_with(pat))
    }

    /// `\name` em i, seguido de não-letra.
    fn starts_cmd(&self, i: usize, name: &str) -> bool {
        if !self.starts(i, "\\") || !self.starts(i + 1, name) {
            return false;
        }
        let j = i + 1 + name.len();
        j >= self.b.len() || !self.b[j].is_ascii_alphabetic()
    }

    fn cmd_name(&self, i: usize) -> Option<&'a str> {
        if i < self.b.len() && self.b[i] == b'\\' {
            let mut j = i + 1;
            while j < self.b.len() && self.b[j].is_ascii_alphabetic() {
                j += 1;
            }
            if j > i + 1 {
                return Some(&self.s[i + 1..j]);
            }
        }
        None
    }

    fn line_end(&self, i: usize) -> usize {
        self.s[i..].find('\n').map_or(self.b.len(), |k| i + k)
    }
    fn after_line(&self, i: usize) -> usize {
        let e = self.line_end(i);
        if e < self.b.len() { e + 1 } else { e }
    }
    fn only_ws(&self, a: usize, e: usize) -> bool {
        self.b[a..e].iter().all(|c| matches!(c, b' ' | b'\t' | b'\r'))
    }
    fn skip_sp(&self, mut i: usize) -> usize {
        while i < self.b.len() && matches!(self.b[i], b' ' | b'\t') {
            i += 1;
        }
        i
    }
    /// Espaços até o fim da linha (inclui o \n) se a linha só tem espaços.
    fn eat_rest_ws(&self, k: usize) -> usize {
        let j = self.skip_sp(k);
        if j >= self.b.len() {
            j
        } else if self.b[j] == b'\n' {
            j + 1
        } else if self.b[j] == b'\r' && self.starts(j, "\r\n") {
            j + 2
        } else {
            k
        }
    }
    /// Fim de bloco de texto: espaços e um \n.
    fn eat_post(&self, k: usize) -> usize {
        let j = self.skip_sp(k);
        if j < self.b.len() && self.b[j] == b'\n' {
            j + 1
        } else if j >= self.b.len() {
            j
        } else {
            k
        }
    }

    /// i aponta para '{'; devolve o índice após o '}' correspondente.
    pub fn match_brace(&self, i: usize, lim: usize) -> Option<usize> {
        let b = self.b;
        let mut d = 0usize;
        let mut j = i;
        while j < lim {
            match b[j] {
                b'\\' => j += 1,
                b'%' => {
                    while j < lim && b[j] != b'\n' {
                        j += 1;
                    }
                }
                b'{' => d += 1,
                b'}' => {
                    d -= 1;
                    if d == 0 {
                        return Some(j + 1);
                    }
                }
                _ => {}
            }
            j += 1;
        }
        None
    }

    /// i aponta para '['; não cruza linha em branco.
    fn match_bracket(&self, i: usize, lim: usize) -> Option<usize> {
        let b = self.b;
        let mut d = 0i32;
        let mut j = i + 1;
        while j < lim {
            match b[j] {
                b'\\' => j += 1,
                b'{' => d += 1,
                b'}' => d -= 1,
                b']' if d == 0 => return Some(j + 1),
                b'\n' => {
                    let k = self.skip_sp(j + 1);
                    if k < lim && b[k] == b'\n' {
                        return None;
                    }
                }
                _ => {}
            }
            if d < 0 {
                return None;
            }
            j += 1;
        }
        None
    }

    /// q aponta para `\begin{name}`; devolve índice após `\end{name}`.
    fn env_end(&self, q: usize, lim: usize) -> Option<usize> {
        let ne = self.s[q + 7..lim].find('}')? + q + 7;
        let name = &self.s[q + 7..ne];
        let open = format!("\\begin{{{}}}", name);
        let close = format!("\\end{{{}}}", name);
        let mut d = 1;
        let mut j = ne + 1;
        while j < lim {
            let r = &self.s[j..lim];
            let o = r.find(&open);
            let c = r.find(&close)?;
            match o {
                Some(o) if o < c => {
                    d += 1;
                    j += o + open.len();
                }
                _ => {
                    d -= 1;
                    j += c + close.len();
                    if d == 0 {
                        return Some(j);
                    }
                }
            }
        }
        None
    }

    /// Lê argumentos [..] e {..} imediatamente adjacentes.
    fn read_args(&self, mut j: usize, lim: usize) -> (Vec<(bool, usize, usize)>, usize) {
        let mut args = vec![];
        while j < lim && args.len() < 9 {
            if self.b[j] == b'{' {
                match self.match_brace(j, lim) {
                    Some(k) => {
                        args.push((false, j + 1, k - 1));
                        j = k;
                    }
                    None => break,
                }
            } else if self.b[j] == b'[' {
                match self.match_bracket(j, lim) {
                    Some(k) => {
                        args.push((true, j + 1, k - 1));
                        j = k;
                    }
                    None => break,
                }
            } else {
                break;
            }
        }
        (args, j)
    }

    fn heading_at(&self, q: usize) -> Option<(u8, usize)> {
        let name = self.cmd_name(q)?;
        let lvl = HEADINGS.iter().find(|(n, _)| *n == name)?.1;
        let mut k = q + 1 + name.len();
        if k < self.b.len() && self.b[k] == b'*' {
            k += 1;
        }
        if k < self.b.len() && (self.b[k] == b'{' || self.b[k] == b'[') {
            Some((lvl, k))
        } else {
            None
        }
    }

    /// Algo em j (início de linha) que começa um novo bloco?
    fn structural_at(&self, j: usize) -> bool {
        let j = self.skip_sp(j);
        if j >= self.b.len() {
            return true;
        }
        if self.b[j] == b'%' {
            return true;
        }
        if self.starts(j, "\\begin{") || self.starts(j, "\\end{") || self.starts_cmd(j, "item") || self.starts(j, "\\[") {
            return true;
        }
        if self.heading_at(j).is_some() {
            return true;
        }
        if let Some(n) = self.cmd_name(j) {
            if STRUCT_CMDS.contains(&n) {
                return true;
            }
        }
        false
    }

    fn find_from(&self, i: usize, lim: usize, pat: &str) -> Option<usize> {
        self.s[i..lim].find(pat).map(|k| i + k)
    }

    /// `$...$` sem cruzar linha em branco.
    fn find_dollar(&self, i: usize, lim: usize) -> Option<usize> {
        let b = self.b;
        let mut j = i;
        while j < lim {
            match b[j] {
                b'\\' => j += 1,
                b'$' => return Some(j),
                b'\n' => {
                    let k = self.skip_sp(j + 1);
                    if k < lim && b[k] == b'\n' {
                        return None;
                    }
                }
                _ => {}
            }
            j += 1;
        }
        None
    }

    fn text_arg(&self, name: &str, a: usize, e: usize) -> bool {
        if NO_TEXT_ARGS.contains(&name) {
            return false;
        }
        let t = &self.s[a..e];
        t.contains(' ') && t.chars().count() >= 12
    }

    fn inl(&self, mut i: usize, lim: usize, mode: Mode, in_list: bool) -> (Vec<Inline>, usize) {
        let s = self.s;
        let b = self.b;
        let top = mode != Mode::Inner;
        let mut out: Vec<Inline> = vec![];
        let mut tb = Tb::default();
        while i < lim {
            let c = b[i];
            match c {
                b' ' | b'\t' | b'\n' | b'\r' => {
                    let mut j = i;
                    while j < lim && matches!(b[j], b' ' | b'\t' | b'\n' | b'\r') {
                        j += 1;
                    }
                    let run = &s[i..j];
                    let nl = run.matches('\n').count();
                    if top && (nl >= 2 || j >= lim || (nl == 1 && self.structural_at(j))) {
                        break;
                    }
                    tb.push(' ', run);
                    i = j;
                }
                b'{' => match self.match_brace(i, lim) {
                    Some(k) => {
                        tb.flush(&mut out);
                        let (c, _) = self.inl(i + 1, k - 1, Mode::Inner, in_list);
                        out.push(Inline::Group { c });
                        i = k;
                    }
                    None => {
                        tb.push('{', "{");
                        i += 1;
                    }
                },
                b'}' => {
                    tb.flush(&mut out);
                    out.push(atom("}", "cmd", None));
                    i += 1;
                }
                b'%' => {
                    let k = self.line_end(i).min(lim);
                    tb.flush(&mut out);
                    out.push(atom(&s[i..k], "comment", None));
                    i = k;
                }
                b'$' => {
                    if i + 1 < lim && b[i + 1] == b'$' {
                        if let Some(k) = self.find_from(i + 2, lim, "$$") {
                            tb.flush(&mut out);
                            out.push(atom(&s[i..k + 2], "dmath", None));
                            i = k + 2;
                            continue;
                        }
                    } else if let Some(k) = self.find_dollar(i + 1, lim) {
                        tb.flush(&mut out);
                        out.push(atom(&s[i..k + 1], "math", None));
                        i = k + 1;
                        continue;
                    }
                    tb.push('$', "$");
                    i += 1;
                }
                b'~' => {
                    tb.push('\u{a0}', "~");
                    i += 1;
                }
                b'`' => {
                    if i + 1 < lim && b[i + 1] == b'`' {
                        tb.push('“', "``");
                        i += 2;
                    } else {
                        tb.push('‘', "`");
                        i += 1;
                    }
                }
                b'\'' => {
                    if i + 1 < lim && b[i + 1] == b'\'' {
                        tb.push('”', "''");
                        i += 2;
                    } else {
                        tb.push('\'', "'");
                        i += 1;
                    }
                }
                b'-' => {
                    if self.starts(i, "---") && i + 3 <= lim {
                        tb.push('—', "---");
                        i += 3;
                    } else if self.starts(i, "--") && i + 2 <= lim {
                        tb.push('–', "--");
                        i += 2;
                    } else {
                        tb.push('-', "-");
                        i += 1;
                    }
                }
                b'\\' => {
                    if i + 1 >= lim {
                        tb.push('\\', "\\");
                        i += 1;
                        continue;
                    }
                    let n = b[i + 1];
                    if n.is_ascii_alphabetic() {
                        let mut j = i + 1;
                        while j < lim && b[j].is_ascii_alphabetic() {
                            j += 1;
                        }
                        let name = &s[i + 1..j];
                        let mut je = j;
                        if je < lim && b[je] == b'*' {
                            je += 1;
                        }
                        if top && (name == "end" || (name == "item" && (in_list || mode == Mode::Item))) {
                            break;
                        }
                        tb.flush(&mut out);
                        if name == "begin" {
                            match self.env_end(i, lim) {
                                Some(k) => {
                                    out.push(atom(&s[i..k], "env", None));
                                    i = k;
                                }
                                None => {
                                    out.push(atom(&s[i..j], "cmd", None));
                                    i = j;
                                }
                            }
                            continue;
                        }
                        if FMT.contains(&name) && j < lim && b[j] == b'{' {
                            if let Some(k) = self.match_brace(j, lim) {
                                let (c, _) = self.inl(j + 1, k - 1, Mode::Inner, in_list);
                                out.push(Inline::Fmt { cmd: name.to_string(), c });
                                i = k;
                                continue;
                            }
                        }
                        let (args, k) = self.read_args(je, lim);
                        let kind = match name {
                            "ce" => Some("ce"),
                            "SI" | "si" | "qty" | "unit" | "num" | "ang" | "SIrange" | "numrange" => Some("si"),
                            "includegraphics" => Some("img"),
                            _ if REFS.contains(&name) => Some("ref"),
                            _ if LAYOUT.contains(&name) => Some("layout"),
                            _ => None,
                        };
                        if let Some(kind) = kind {
                            let mut k2 = k;
                            if args.is_empty() {
                                k2 = self.skip_sp(k).min(lim);
                            }
                            out.push(atom(&s[i..k2], kind, None));
                            i = k2;
                            continue;
                        }
                        if args.is_empty() {
                            let k2 = self.skip_sp(je).min(lim);
                            match sym(name) {
                                Some(d) => out.push(atom(&s[i..k2], "sym", Some(d))),
                                None => out.push(atom(&s[i..k2], "cmd", None)),
                            }
                            i = k2;
                            continue;
                        }
                        let has_text = args.iter().any(|&(o, a, e)| !o && self.text_arg(name, a, e));
                        if !has_text {
                            out.push(atom(&s[i..k], "cmd", None));
                            i = k;
                            continue;
                        }
                        let margs = args
                            .iter()
                            .map(|&(o, a, e)| {
                                if !o && self.text_arg(name, a, e) {
                                    Arg { opt: false, raw: None, c: Some(self.inl(a, e, Mode::Inner, in_list).0) }
                                } else {
                                    Arg { opt: o, raw: Some(s[a..e].to_string()), c: None }
                                }
                            })
                            .collect();
                        out.push(Inline::Macro { name: s[i..je].to_string(), args: margs });
                        i = k;
                    } else {
                        match n {
                            b'%' | b'&' | b'#' | b'_' | b'$' | b'{' | b'}' => {
                                tb.push(n as char, &s[i..i + 2]);
                                i += 2;
                            }
                            b',' => {
                                tb.push('\u{2009}', "\\,");
                                i += 2;
                            }
                            b';' | b':' | b'>' => {
                                tb.push('\u{2005}', &s[i..i + 2]);
                                i += 2;
                            }
                            b' ' | b'\n' => {
                                tb.push(' ', &s[i..i + 2]);
                                i += 2;
                            }
                            b'\\' => {
                                let mut k = i + 2;
                                if k < lim && b[k] == b'*' {
                                    k += 1;
                                }
                                if k < lim && b[k] == b'[' {
                                    if let Some(m) = self.match_bracket(k, lim) {
                                        k = m;
                                    }
                                }
                                tb.flush(&mut out);
                                out.push(atom(&s[i..k], "br", None));
                                i = k;
                            }
                            b'(' | b'[' => {
                                let (close, kind) = if n == b'(' { ("\\)", "math") } else { ("\\]", "dmath") };
                                tb.flush(&mut out);
                                match self.find_from(i + 2, lim, close) {
                                    Some(k) => {
                                        out.push(atom(&s[i..k + 2], kind, None));
                                        i = k + 2;
                                    }
                                    None => {
                                        out.push(atom(&s[i..i + 2], "cmd", None));
                                        i += 2;
                                    }
                                }
                            }
                            _ if combining(n).is_some() => {
                                let mut k = i + 2;
                                let base: String;
                                if k < lim && b[k] == b'{' {
                                    let m = self.match_brace(k, lim).unwrap_or(k + 1);
                                    base = s[k + 1..m.saturating_sub(1).max(k + 1)].to_string();
                                    k = m;
                                } else if k < lim {
                                    let ch = s[k..].chars().next().unwrap();
                                    base = ch.to_string();
                                    k += ch.len_utf8();
                                } else {
                                    base = String::new();
                                }
                                let mut d = base.replace("\\i", "ı");
                                d.push(combining(n).unwrap());
                                tb.flush(&mut out);
                                out.push(atom(&s[i..k], "sym", Some(&d)));
                                i = k;
                            }
                            _ => {
                                let ch = s[i + 1..].chars().next().unwrap();
                                let k = i + 1 + ch.len_utf8();
                                tb.flush(&mut out);
                                let kind = if matches!(n, b'-' | b'/' | b'@' | b'!') { "layout" } else { "cmd" };
                                out.push(atom(&s[i..k], kind, None));
                                i = k;
                            }
                        }
                    }
                }
                _ => {
                    let ch = s[i..].chars().next().unwrap();
                    let k = i + ch.len_utf8();
                    tb.push(ch, &s[i..k]);
                    i = k;
                }
            }
        }
        tb.flush(&mut out);
        (out, i)
    }
}

/// Analisa um fragmento isolado (célula de tabela, título de caixa).
pub fn parse_fragment(s: &str) -> Vec<Inline> {
    P::new(s).inl(0, s.len(), Mode::Inner, false).0
}

pub fn parse(s: &str) -> Vec<Block> {
    let p = P::new(s);
    let b = s.as_bytes();
    let n = s.len();
    let mut out: Vec<Block> = vec![];
    let mut stack: Vec<String> = vec![];
    let mut i = 0;
    let mk = |kind: &'static str, a: usize, e: usize, depth: usize| Block {
        kind,
        start: a,
        end: e,
        depth,
        src: s[a..e].to_string(),
        ..Default::default()
    };
    while i < n {
        let start = i;
        let depth = stack.len();
        let le = p.line_end(i);
        if p.only_ws(i, le) {
            let mut j = if le < n { le + 1 } else { n };
            while j < n {
                let le2 = p.line_end(j);
                if p.only_ws(j, le2) {
                    j = if le2 < n { le2 + 1 } else { n };
                } else {
                    break;
                }
            }
            out.push(mk("blank", start, j, depth));
            i = j;
            continue;
        }
        let q = p.skip_sp(i);

        if b[q] == b'%' {
            let mut j = p.after_line(q);
            while j < n {
                let r = p.skip_sp(j);
                if r < n && b[r] == b'%' {
                    j = p.after_line(r);
                } else {
                    break;
                }
            }
            out.push(mk("comment", start, j, depth));
            i = j;
            continue;
        }

        if p.starts(q, "\\begin{") {
            if let Some(ne) = s[q + 7..].find('}').map(|k| k + q + 7) {
                let name = s[q + 7..ne].to_string();
                let raw = RAW_ENVS.contains(&name.as_str());
                let math = MATH_ENVS.contains(&name.as_str());
                if raw || math {
                    let e = p.env_end(q, n).unwrap_or(n);
                    let e2 = p.eat_rest_ws(e);
                    let mut blk = mk(if math { "math" } else { "raw" }, start, e2, depth);
                    blk.env = name;
                    out.push(blk);
                    i = e2;
                    continue;
                }
                let mut k = ne + 1;
                let mut opt = None;
                loop {
                    if k < n && b[k] == b'[' {
                        match p.match_bracket(k, n) {
                            Some(m) => {
                                if opt.is_none() {
                                    opt = Some(s[k + 1..m - 1].to_string());
                                }
                                k = m;
                            }
                            None => break,
                        }
                    } else if k < n && b[k] == b'{' && BRACE_ARG_ENVS.contains(&name.as_str()) {
                        match p.match_brace(k, n) {
                            Some(m) => k = m,
                            None => break,
                        }
                    } else {
                        break;
                    }
                }
                let e2 = p.eat_rest_ws(k);
                let mut blk = mk("begin", start, e2, depth);
                blk.env = name.clone();
                blk.opt = opt;
                out.push(blk);
                stack.push(name);
                i = e2;
                continue;
            }
        }

        if p.starts(q, "\\end{") {
            let k = s[q..].find('}').map_or(n, |k| q + k + 1);
            let name = s[q + 5..k.saturating_sub(1).max(q + 5)].to_string();
            stack.pop();
            let e2 = p.eat_rest_ws(k);
            let mut blk = mk("end", start, e2, stack.len());
            blk.env = name;
            out.push(blk);
            i = e2;
            continue;
        }

        let in_list = stack.last().map_or(false, |e| LIST_ENVS.contains(&e.as_str()));

        if p.starts_cmd(q, "item") {
            let mut k = q + 5;
            let mut opt = None;
            if k < n && b[k] == b'[' {
                if let Some(m) = p.match_bracket(k, n) {
                    opt = Some(s[k + 1..m - 1].to_string());
                    k = m;
                }
            }
            k = p.skip_sp(k);
            if k < n && b[k] == b'\n' {
                let k2 = p.skip_sp(k + 1);
                if k2 < n && b[k2] != b'\n' && !p.structural_at(k2) {
                    k = k2;
                }
            }
            let (inl, ce) = p.inl(k, n, Mode::Item, true);
            let pe = p.eat_post(ce);
            let mut blk = mk("item", start, pe, depth);
            blk.pre = s[start..k].to_string();
            blk.post = s[ce..pe].to_string();
            blk.inl = inl;
            blk.opt = opt;
            out.push(blk);
            i = pe;
            continue;
        }

        if let Some((level, mut k)) = p.heading_at(q) {
            if b[k] == b'[' {
                if let Some(m) = p.match_bracket(k, n) {
                    k = m;
                }
            }
            if k < n && b[k] == b'{' {
                if let Some(m) = p.match_brace(k, n) {
                    let (inl, _) = p.inl(k + 1, m - 1, Mode::Inner, false);
                    let pe = p.after_line(m - 1);
                    let mut blk = mk("heading", start, pe, depth);
                    blk.level = level;
                    blk.env = p.cmd_name(q).unwrap_or("").to_string();
                    if s[q..k].contains('*') {
                        blk.env.push('*');
                    }
                    blk.pre = s[start..k + 1].to_string();
                    blk.post = s[m - 1..pe].to_string();
                    blk.inl = inl;
                    out.push(blk);
                    i = pe;
                    continue;
                }
            }
        }

        if p.starts(q, "\\[") || p.starts(q, "$$") {
            let close = if p.starts(q, "\\[") { "\\]" } else { "$$" };
            if let Some(e) = p.find_from(q + 2, n, close) {
                let e2 = p.eat_rest_ws(e + 2);
                out.push(mk("math", start, e2, depth));
                i = e2;
                continue;
            }
        }

        if let Some(name) = p.cmd_name(q) {
            if STRUCT_CMDS.contains(&name) {
                let mut k = q + 1 + name.len();
                if k < n && b[k] == b'*' {
                    k += 1;
                }
                if name == "def" || name == "let" {
                    // \def\nome#1#2{...}
                    if let Some(m) = p.cmd_name(k) {
                        k += 1 + m.len();
                    }
                    while k < n && b[k] != b'{' && b[k] != b'\n' {
                        k += 1;
                    }
                }
                let (args, k2) = p.read_args(k, n);
                let e2 = p.eat_rest_ws(k2);
                let mut blk = mk("cmd", start, e2, depth);
                blk.env = name.to_string();
                if (name == "input" || name == "include") && !args.is_empty() {
                    let (_, a, e) = args[0];
                    blk.target = Some(s[a..e].to_string());
                }
                out.push(blk);
                i = e2;
                continue;
            }
        }

        let (inl, ce) = p.inl(q, n, Mode::Para, in_list);
        if ce <= q {
            let e = p.after_line(q).max(q + 1);
            out.push(mk("raw", start, e, depth));
            i = e;
            continue;
        }
        let pe = p.eat_post(ce);
        let mut blk = mk("para", start, pe, depth);
        blk.pre = s[start..q].to_string();
        blk.post = s[ce..pe].to_string();
        blk.inl = inl;
        out.push(blk);
        i = pe;
    }

    for (k, blk) in out.iter_mut().enumerate() {
        blk.id = k;
        if blk.is_text() {
            // Garantia de fidelidade: o modelo tem que reproduzir o fonte.
            let back = format!("{}{}{}", blk.pre, serialize(&blk.inl, "", None).0, blk.post);
            if back != blk.src {
                blk.locked = true;
            }
        }
        if blk.kind == "raw" {
            blk.table = parse_table(&blk.src);
            if let Some(a) = blk.src.find("\\includegraphics") {
                let r = &blk.src[a..];
                if let Some(o) = r.find('{') {
                    if let Some(c) = r[o..].find('}') {
                        blk.target = Some(r[o + 1..o + c].to_string());
                    }
                }
            }
        }
    }
    out
}

// ------------------------------------------------------------- tabelas

fn parse_table(src: &str) -> Option<Vec<Vec<Cell>>> {
    let a = src.find("\\begin{tabular")?;
    let p = P::new(src);
    let mut k = src[a..].find('}')? + a + 1;
    if src[k..].starts_with('{') && src.contains("\\begin{tabular*}") {
        k = p.match_brace(k, src.len())?;
    }
    if src[k..].starts_with('[') {
        k = p.match_bracket(k, src.len())?;
    }
    if !src[k..].starts_with('{') {
        return None;
    }
    k = p.match_brace(k, src.len())?;
    let end = src[k..].find("\\end{tabular")? + k;
    let body = &src[k..end];
    let bb = body.as_bytes();
    let mut rows: Vec<Vec<String>> = vec![vec![]];
    let mut cell = String::new();
    let mut d = 0i32;
    let mut i = 0;
    while i < bb.len() {
        let c = bb[i];
        if c == b'\\' && i + 1 < bb.len() && bb[i + 1] == b'\\' && d == 0 {
            rows.last_mut().unwrap().push(std::mem::take(&mut cell));
            rows.push(vec![]);
            i += 2;
            if i < bb.len() && bb[i] == b'[' {
                while i < bb.len() && bb[i] != b']' {
                    i += 1;
                }
                i += 1;
            }
            continue;
        }
        if c == b'\\' && i + 1 < bb.len() {
            let ch = body[i + 1..].chars().next().unwrap();
            cell.push_str(&body[i..i + 1 + ch.len_utf8()]);
            i += 1 + ch.len_utf8();
            continue;
        }
        if c == b'{' {
            d += 1;
        }
        if c == b'}' {
            d -= 1;
        }
        if c == b'&' && d == 0 {
            rows.last_mut().unwrap().push(std::mem::take(&mut cell));
            i += 1;
            continue;
        }
        let ch = body[i..].chars().next().unwrap();
        cell.push(ch);
        i += ch.len_utf8();
    }
    rows.last_mut().unwrap().push(cell);
    let rule = |t: &str| -> String {
        let mut t = t.trim().to_string();
        loop {
            let before = t.clone();
            for r in ["\\hline", "\\toprule", "\\midrule", "\\bottomrule", "\\endhead", "\\endfirsthead"] {
                if let Some(rest) = t.strip_prefix(r) {
                    t = rest.trim_start().to_string();
                }
            }
            if t.starts_with("\\cline") || t.starts_with("\\cmidrule") {
                if let Some(c) = t.find('}') {
                    t = t[c + 1..].trim_start().to_string();
                }
            }
            if t == before {
                break;
            }
        }
        t
    };
    let mut out = vec![];
    for r in rows {
        let cells: Vec<String> = r.iter().map(|c| rule(c)).collect();
        if cells.iter().all(|c| c.is_empty()) {
            continue;
        }
        let row = cells
            .iter()
            .map(|c| {
                if c.starts_with("\\multicolumn") {
                    let p = P::new(c);
                    let (args, _) = p.read_args("\\multicolumn".len(), c.len());
                    if args.len() >= 3 {
                        let span = c[args[0].1..args[0].2].trim().parse().unwrap_or(1);
                        return Cell { span, inl: parse_fragment(&c[args[2].1..args[2].2]) };
                    }
                }
                Cell { span: 1, inl: parse_fragment(c) }
            })
            .collect();
        out.push(row);
    }
    if out.is_empty() { None } else { Some(out) }
}

// --------------------------------------------------------- serialização

pub fn escape(c: char) -> String {
    match c {
        '%' => "\\%".into(),
        '&' => "\\&".into(),
        '#' => "\\#".into(),
        '_' => "\\_".into(),
        '$' => "\\$".into(),
        '{' => "\\{".into(),
        '}' => "\\}".into(),
        '\\' => "\\textbackslash{}".into(),
        '~' => "\\textasciitilde{}".into(),
        '^' => "\\textasciicircum{}".into(),
        '—' => "---".into(),
        '–' => "--".into(),
        '“' => "``".into(),
        '”' => "''".into(),
        '‘' => "`".into(),
        '’' => "'".into(),
        '\u{a0}' => "~".into(),
        '\u{2009}' | '\u{202f}' => "\\,".into(),
        '\u{2005}' => "\\;".into(),
        '\n' | '\t' | '\r' => " ".into(),
        c => c.to_string(),
    }
}

struct Ser<'a> {
    out: String,
    par: &'a str,
    caret: Option<usize>,
    pos: Option<Vec<usize>>,
    open: Vec<String>, // aberturas ativas (para partir \textbf{} num Enter)
    in_macro: usize,
    skip_ws: bool, // depois de um Enter, descarta o espaço que sobrou
}

impl<'a> Ser<'a> {
    fn run(&mut self, nodes: &[Inline]) {
        for n in nodes {
            match n {
                Inline::Text { v, segs } => {
                    for (k, ch) in v.chars().enumerate() {
                        if self.skip_ws {
                            if ch == ' ' || ch == '\t' {
                                continue;
                            }
                            self.skip_ws = false;
                        }
                        if let Some(p) = self.pos.as_mut() {
                            p.push(self.out.len());
                        }
                        match segs.get(k) {
                            Some(sg) => self.out.push_str(sg),
                            None => self.out.push_str(&escape(ch)),
                        }
                    }
                }
                Inline::Fmt { cmd, c } => {
                    let o = format!("\\{}{{", cmd);
                    self.out.push_str(&o);
                    self.open.push(o);
                    self.run(c);
                    self.open.pop();
                    self.out.push('}');
                }
                Inline::Group { c } => {
                    self.out.push('{');
                    self.open.push("{".into());
                    self.run(c);
                    self.open.pop();
                    self.out.push('}');
                }
                Inline::Macro { name, args } => {
                    self.out.push_str(name);
                    for a in args {
                        if a.opt {
                            self.out.push('[');
                            self.out.push_str(a.raw.as_deref().unwrap_or(""));
                            self.out.push(']');
                        } else {
                            self.out.push('{');
                            match (&a.raw, &a.c) {
                                (Some(r), _) => self.out.push_str(r),
                                (None, Some(c)) => {
                                    self.in_macro += 1;
                                    self.run(c);
                                    self.in_macro -= 1;
                                }
                                _ => {}
                            }
                            self.out.push('}');
                        }
                    }
                }
                Inline::Atom { src, .. } => {
                    self.skip_ws = false;
                    self.out.push_str(src);
                }
                Inline::Par => {
                    if self.in_macro > 0 || self.par.is_empty() {
                        self.out.push(' ');
                    } else {
                        while self.out.ends_with(' ') || self.out.ends_with('\t') {
                            self.out.pop();
                        }
                        self.skip_ws = true;
                        for _ in &self.open {
                            self.out.push('}');
                        }
                        self.out.push_str(self.par);
                        for o in self.open.clone() {
                            self.out.push_str(&o);
                        }
                    }
                }
                Inline::Caret => {
                    if self.caret.is_none() {
                        self.caret = Some(self.out.len());
                    }
                }
            }
        }
    }
}

/// Serializa o modelo. Devolve (fonte, posição do cursor).
pub fn serialize(nodes: &[Inline], par: &str, _x: Option<()>) -> (String, Option<usize>) {
    let mut s = Ser { out: String::new(), par, caret: None, pos: None, open: vec![], in_macro: 0, skip_ws: false };
    s.run(nodes);
    (s.out, s.caret)
}

/// Posição (no fonte do conteúdo) de cada caractere exibido.
pub fn char_positions(nodes: &[Inline]) -> Vec<usize> {
    let mut s = Ser { out: String::new(), par: "", caret: None, pos: Some(vec![]), open: vec![], in_macro: 0, skip_ws: false };
    s.run(nodes);
    s.pos.unwrap()
}

// -------------------------------------------- reaproveitamento de fonte

fn flat_old(nodes: &[Inline], out: &mut Vec<(char, String)>) {
    for n in nodes {
        match n {
            Inline::Text { v, segs } => {
                for (k, ch) in v.chars().enumerate() {
                    out.push((ch, segs.get(k).cloned().unwrap_or_else(|| escape(ch))));
                }
            }
            Inline::Fmt { c, .. } | Inline::Group { c } => flat_old(c, out),
            Inline::Macro { args, .. } => {
                for a in args {
                    if let Some(c) = &a.c {
                        flat_old(c, out);
                    }
                }
            }
            _ => {}
        }
    }
}

fn flat_new(nodes: &[Inline], out: &mut Vec<char>) {
    for n in nodes {
        match n {
            Inline::Text { v, .. } => out.extend(v.chars()),
            Inline::Fmt { c, .. } | Inline::Group { c } => flat_new(c, out),
            Inline::Macro { args, .. } => {
                for a in args {
                    if let Some(c) = &a.c {
                        flat_new(c, out);
                    }
                }
            }
            _ => {}
        }
    }
}

pub fn flat_text(nodes: &[Inline]) -> String {
    let mut v = vec![];
    flat_new(nodes, &mut v);
    v.into_iter().collect()
}

fn assign(nodes: &mut [Inline], segs: &mut std::vec::IntoIter<String>) {
    for n in nodes {
        match n {
            Inline::Text { v, segs: sg } => {
                *sg = v.chars().map(|ch| segs.next().unwrap_or_else(|| escape(ch))).collect();
            }
            Inline::Fmt { c, .. } | Inline::Group { c } => assign(c, segs),
            Inline::Macro { args, .. } => {
                for a in args {
                    if let Some(c) = &mut a.c {
                        assign(c, segs);
                    }
                }
            }
            _ => {}
        }
    }
}

/// Para cada caractere novo, escolhe o fonte: reaproveita o do caractere
/// antigo correspondente (prefixo, sufixo e LCS do miolo); o resto é
/// escapado. Ex.: um "\n" no meio do parágrafo continua "\n" se não mudou.
fn align(old: &[(char, String)], new: &[char]) -> Vec<String> {
    let (ol, nl) = (old.len(), new.len());
    let mut p = 0;
    while p < ol && p < nl && old[p].0 == new[p] {
        p += 1;
    }
    let mut sfx = 0;
    while sfx < ol - p && sfx < nl - p && old[ol - 1 - sfx].0 == new[nl - 1 - sfx] {
        sfx += 1;
    }
    let mut res: Vec<String> = Vec::with_capacity(nl);
    for item in old.iter().take(p) {
        res.push(item.1.clone());
    }
    let om = &old[p..ol - sfx];
    let nm = &new[p..nl - sfx];
    let mut mid: Vec<Option<String>> = vec![None; nm.len()];
    if !om.is_empty() && !nm.is_empty() && om.len() * nm.len() <= 4_000_000 {
        let (a, b) = (om.len(), nm.len());
        let w = b + 1;
        let mut dp = vec![0u32; (a + 1) * w];
        for x in (0..a).rev() {
            for y in (0..b).rev() {
                dp[x * w + y] = if om[x].0 == nm[y] {
                    dp[(x + 1) * w + y + 1] + 1
                } else {
                    dp[(x + 1) * w + y].max(dp[x * w + y + 1])
                };
            }
        }
        let (mut x, mut y) = (0, 0);
        while x < a && y < b {
            if om[x].0 == nm[y] {
                mid[y] = Some(om[x].1.clone());
                x += 1;
                y += 1;
            } else if dp[(x + 1) * w + y] >= dp[x * w + y + 1] {
                x += 1;
            } else {
                y += 1;
            }
        }
    }
    for (k, m) in mid.into_iter().enumerate() {
        res.push(m.unwrap_or_else(|| escape(nm[k])));
    }
    for item in old.iter().skip(ol - sfx) {
        res.push(item.1.clone());
    }
    res
}

/// Recebe o modelo editado (do navegador) e devolve o novo fonte do
/// conteúdo do bloco, reaproveitando ao máximo o fonte antigo.
pub fn rebuild(old: &[Inline], new: &mut Vec<Inline>, par: &str) -> (String, Option<usize>) {
    let mut of = vec![];
    flat_old(old, &mut of);
    let mut nf = vec![];
    flat_new(new, &mut nf);
    let segs = align(&of, &nf);
    assign(new, &mut segs.into_iter());
    serialize(new, par, None)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn roundtrip(s: &str) {
        let blocks = parse(s);
        let cat: String = blocks.iter().map(|b| b.src.as_str()).collect();
        assert_eq!(cat, s);
        for b in &blocks {
            assert!(!b.locked, "bloco travado: {:?}", b.src);
        }
    }

    #[test]
    fn basic() {
        roundtrip("\\chapter{A química --- água}\n\nTexto ``aspas'' com 60\\,\\% e $x^2$.\n\\begin{itemize}[nosep]\n\\item Um \\textbf{dois};\n\\item Três\n\\end{itemize}\n");
        roundtrip("\\begin{resolvido}[T]\nA \\estrategia{Aplique os fatores a cada grama} b\n\\resolucao\n(a) x.\\end{resolvido}\n");
        roundtrip("{\\small Dados: TACO (2011), n\\textsuperscript{o}~9}\n% coment\n\\section{X}\\label{a}\n");
    }

    #[test]
    fn edit_keeps_newlines() {
        let s = "Linha um\ncontinua aqui.\n";
        let b = &parse(s)[0];
        let mut new = vec![Inline::Text { v: "Linha um continua ali.".into(), segs: vec![] }];
        let (out, _) = rebuild(&b.inl, &mut new, "\n\n");
        assert_eq!(out, "Linha um\ncontinua ali.");
    }

    #[test]
    fn bold_split() {
        let s = "Um \\textbf{dois três} quatro\n";
        let b = &parse(s)[0];
        let mut new = vec![
            Inline::Text { v: "Um ".into(), segs: vec![] },
            Inline::Fmt { cmd: "textbf".into(), c: vec![
                Inline::Text { v: "dois".into(), segs: vec![] },
                Inline::Par,
                Inline::Caret,
                Inline::Text { v: " três".into(), segs: vec![] },
            ] },
            Inline::Text { v: " quatro".into(), segs: vec![] },
        ];
        let (out, caret) = rebuild(&b.inl, &mut new, "\n\n");
        assert_eq!(out, "Um \\textbf{dois}\n\n\\textbf{três} quatro");
        assert_eq!(caret, Some("Um \\textbf{dois}\n\n\\textbf{".len()));
    }
}
