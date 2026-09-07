use std::collections::BTreeMap;
use std::io::{Cursor, Write};
use zip::write::SimpleFileOptions;
use zip::ZipWriter;

const MAX_OUT: usize = 20_000;
const PY_WRAP: &str = r#"
_buf = []
def print(*a, sep=" ", end="\n", file=None, flush=False):
    _buf.append(sep.join([str(x) for x in a]) + end)
"#;

#[derive(Clone, Default)]
pub struct Vfs {
    files: BTreeMap<String, String>,
}

impl Vfs {
    pub fn list(&self) -> String {
        let names: Vec<&str> = self
            .files
            .keys()
            .filter(|k| !k.ends_with("/.keep"))
            .map(|k| k.as_str())
            .collect();
        if names.is_empty() {
            return "(empty)".into();
        }
        names.join("\n")
    }

    pub fn zip(&self) -> Result<Vec<u8>, String> {
        let mut buf = Cursor::new(Vec::new());
        let mut zw = ZipWriter::new(&mut buf);
        let opts = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
        for (name, content) in &self.files {
            if name.ends_with("/.keep") && content.is_empty() {
                let dir = name.trim_end_matches(".keep");
                zw.add_directory(dir, opts).map_err(|e| e.to_string())?;
                continue;
            }
            zw.start_file(name, opts).map_err(|e| e.to_string())?;
            zw.write_all(content.as_bytes()).map_err(|e| e.to_string())?;
        }
        zw.finish().map_err(|e| e.to_string())?;
        Ok(buf.into_inner())
    }

    pub fn norm(path: &str) -> Result<String, String> {
        let raw = path.trim().trim_start_matches('/');
        if raw.is_empty() {
            return Err("empty path".into());
        }
        let mut out: Vec<&str> = Vec::new();
        for part in raw.split('/') {
            match part {
                "" | "." => {}
                ".." => {
                    out.pop();
                }
                p if p.contains('\0') => return Err("bad path".into()),
                p => out.push(p),
            }
        }
        if out.is_empty() {
            return Err("empty path".into());
        }
        Ok(out.join("/"))
    }

    pub fn read(&self, path: &str) -> Result<String, String> {
        let p = Self::norm(path)?;
        if let Some(text) = self.files.get(&p) {
            return Ok(numbered(text));
        }
        let prefix = format!("{p}/");
        let kids: Vec<_> = self
            .files
            .keys()
            .filter(|k| k.starts_with(&prefix) || *k == &p)
            .cloned()
            .collect();
        if kids.is_empty() {
            Err(format!("not found: {p}"))
        } else {
            Ok(kids.join("\n"))
        }
    }

    pub fn read_raw(&self, path: &str) -> Result<String, String> {
        let p = Self::norm(path)?;
        self.files
            .get(&p)
            .cloned()
            .ok_or_else(|| format!("not found: {p}"))
    }

    pub fn write(&mut self, path: &str, contents: &str) -> Result<String, String> {
        let p = Self::norm(path)?;
        self.files.insert(p.clone(), contents.to_string());
        Ok(format!("wrote {p} ({} bytes)", contents.len()))
    }

    pub fn edit(&mut self, path: &str, old: &str, new: &str) -> Result<String, String> {
        let p = Self::norm(path)?;
        let text = self
            .files
            .get(&p)
            .ok_or_else(|| format!("not found: {p}"))?;
        let n = text.matches(old).count();
        if n == 0 {
            return Err(format!("old not found in {p}"));
        }
        if n > 1 {
            return Err(format!("old found {n} times in {p}; must appear once"));
        }
        let next = text.replacen(old, new, 1);
        self.files.insert(p.clone(), next);
        Ok(format!("edited {p}"))
    }

    pub fn bash(&mut self, cmd: &str) -> Result<String, String> {
        run(self, cmd)
    }
}

fn numbered(text: &str) -> String {
    text.lines()
        .enumerate()
        .map(|(i, line)| format!("{:>4}|{line}", i + 1))
        .collect::<Vec<_>>()
        .join("\n")
}

fn cap(mut s: String) -> String {
    if s.len() > MAX_OUT {
        s.truncate(MAX_OUT);
        s.push_str("\n... (truncated)");
    }
    s
}

#[derive(Clone, Debug)]
struct Cmd {
    argv: Vec<String>,
    stdout_to: Option<String>,
}

fn tokenize(input: &str) -> Result<Vec<String>, String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut chars = input.chars().peekable();
    let mut quote: Option<char> = None;
    while let Some(c) = chars.next() {
        if let Some(q) = quote {
            if c == '\\' && q == '"' {
                if let Some(n) = chars.next() {
                    cur.push(n);
                }
                continue;
            }
            if c == q {
                quote = None;
                continue;
            }
            cur.push(c);
            continue;
        }
        match c {
            '\'' | '"' => quote = Some(c),
            c if c.is_whitespace() => {
                if !cur.is_empty() {
                    out.push(std::mem::take(&mut cur));
                }
            }
            '|' | '>' | ';' => {
                if !cur.is_empty() {
                    out.push(std::mem::take(&mut cur));
                }
                out.push(c.to_string());
            }
            '&' if chars.peek() == Some(&'&') => {
                chars.next();
                if !cur.is_empty() {
                    out.push(std::mem::take(&mut cur));
                }
                out.push("&&".into());
            }
            _ => cur.push(c),
        }
    }
    if quote.is_some() {
        return Err("unclosed quote".into());
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    Ok(out)
}

fn parse_pipeline(tokens: &[String]) -> Result<Vec<Cmd>, String> {
    if tokens.is_empty() {
        return Err("empty cmd".into());
    }
    let mut cmds = Vec::new();
    let mut argv = Vec::new();
    let mut stdout_to = None;
    let mut i = 0usize;
    while i < tokens.len() {
        match tokens[i].as_str() {
            "|" => {
                if argv.is_empty() {
                    return Err("empty command in pipeline".into());
                }
                cmds.push(Cmd {
                    argv: std::mem::take(&mut argv),
                    stdout_to: stdout_to.take(),
                });
            }
            ">" => {
                i += 1;
                let path = tokens.get(i).ok_or("missing redirect path")?;
                stdout_to = Some(path.clone());
            }
            "&&" | ";" => return Err("only | and > are supported in this virtual shell".into()),
            t => argv.push(t.to_string()),
        }
        i += 1;
    }
    if argv.is_empty() {
        return Err("empty command".into());
    }
    cmds.push(Cmd { argv, stdout_to });
    Ok(cmds)
}

fn run(vfs: &mut Vfs, line: &str) -> Result<String, String> {
    let line = line.trim();
    if line.is_empty() {
        return Err("empty cmd".into());
    }
    let tokens = tokenize(line)?;
    let pipeline = parse_pipeline(&tokens)?;
    let mut stdin = String::new();
    let mut last = String::new();
    let n = pipeline.len();
    for (i, cmd) in pipeline.into_iter().enumerate() {
        let out = exec(vfs, &cmd.argv, &stdin)?;
        if let Some(path) = &cmd.stdout_to {
            vfs.write(path, &out)?;
            last = String::new();
            stdin.clear();
        } else if i + 1 < n {
            stdin = out;
            last.clear();
        } else {
            last = out;
        }
    }
    Ok(cap(last))
}

fn exec(vfs: &mut Vfs, argv: &[String], stdin: &str) -> Result<String, String> {
    let Some(name) = argv.first().map(|s| s.as_str()) else {
        return Err("empty command".into());
    };
    let args = &argv[1..];
    match name {
        "pwd" => Ok("/workspace".into()),
        "ls" => Ok(ls(vfs, args)),
        "cat" => cat(vfs, args, stdin),
        "echo" => Ok(echo(args)),
        "mkdir" => mkdir(vfs, args),
        "rm" => rm(vfs, args),
        "head" => Ok(head(stdin, args)),
        "wc" => {
            let text = if args.is_empty() {
                stdin.to_string()
            } else {
                cat(vfs, args, stdin)?
            };
            Ok(wc(&text))
        }
        "python" | "python3" => python(vfs, args, stdin),
        "true" => Ok(String::new()),
        "false" => Err("false".into()),
        other => Err(format!(
            "unknown command: {other}. virtual shell: ls cat pwd mkdir rm echo head wc python. pipes and > work. no real OS."
        )),
    }
}

fn ls(vfs: &Vfs, args: &[String]) -> String {
    if args.is_empty() || args.iter().all(|a| a.starts_with('-')) {
        return vfs.list();
    }
    let mut out = Vec::new();
    for a in args {
        if a.starts_with('-') {
            continue;
        }
        match vfs.read(a) {
            Ok(s) => out.push(s),
            Err(e) => out.push(e),
        }
    }
    out.join("\n")
}

fn cat(vfs: &Vfs, args: &[String], stdin: &str) -> Result<String, String> {
    if args.is_empty() {
        return Ok(stdin.to_string());
    }
    let mut out = String::new();
    for a in args {
        if a == "-" {
            out.push_str(stdin);
            continue;
        }
        out.push_str(&vfs.read_raw(a)?);
        if !out.ends_with('\n') {
            out.push('\n');
        }
    }
    Ok(out)
}

fn echo(args: &[String]) -> String {
    let mut n = false;
    let mut i = 0usize;
    if args.first().map(|s| s.as_str()) == Some("-n") {
        n = true;
        i = 1;
    }
    let mut s = args[i..].join(" ");
    if !n {
        s.push('\n');
    }
    s
}

fn mkdir(vfs: &mut Vfs, args: &[String]) -> Result<String, String> {
    if args.is_empty() {
        return Err("mkdir: missing operand".into());
    }
    for a in args {
        if a.starts_with('-') {
            continue;
        }
        let p = Vfs::norm(a)?;
        vfs.files.entry(format!("{p}/.keep")).or_insert_with(String::new);
    }
    Ok(String::new())
}

fn rm(vfs: &mut Vfs, args: &[String]) -> Result<String, String> {
    let force = args.iter().any(|a| a == "-f" || a == "-rf" || a == "-fr");
    let paths: Vec<_> = args.iter().filter(|a| !a.starts_with('-')).cloned().collect();
    if paths.is_empty() {
        return Err("rm: missing operand".into());
    }
    for p in paths {
        let p = Vfs::norm(&p)?;
        if vfs.files.remove(&p).is_none() && !force {
            return Err(format!("not found: {p}"));
        }
        let prefix = format!("{p}/");
        let kids: Vec<_> = vfs
            .files
            .keys()
            .filter(|k| k.starts_with(&prefix))
            .cloned()
            .collect();
        for k in kids {
            vfs.files.remove(&k);
        }
    }
    Ok(String::new())
}

fn head(stdin: &str, args: &[String]) -> String {
    let n = args
        .iter()
        .find_map(|a| a.strip_prefix("-n").and_then(|s| s.parse().ok()))
        .or_else(|| {
            args.windows(2).find_map(|w| {
                (w[0] == "-n").then(|| w[1].parse().ok()).flatten()
            })
        })
        .unwrap_or(10usize);
    stdin.lines().take(n).collect::<Vec<_>>().join("\n")
}

fn wc(text: &str) -> String {
    let lines = if text.is_empty() {
        0
    } else {
        text.lines().count()
    };
    let words = text.split_whitespace().count();
    let bytes = text.len();
    format!("{lines} {words} {bytes}\n")
}

fn python(vfs: &Vfs, args: &[String], stdin: &str) -> Result<String, String> {
    let mut source = String::new();
    let mut i = 0usize;
    while i < args.len() {
        match args[i].as_str() {
            "-c" => {
                i += 1;
                source = args.get(i).cloned().ok_or("python -c: missing code")?;
                break;
            }
            "-u" | "-B" => {}
            path => {
                source = vfs.read_raw(path)?;
                break;
            }
        }
        i += 1;
    }
    if source.is_empty() {
        source = stdin.to_string();
    }
    if source.trim().is_empty() {
        return Err("python: no script".into());
    }
    run_python(&source)
}

fn run_python(source: &str) -> Result<String, String> {
    use rustpython_vm::{Interpreter, Settings};

    let wrapped = format!(
        "{PY_WRAP}\n{source}\n__fun_out = ''.join(_buf)\n"
    );
    let settings = Settings::default();
    Interpreter::without_stdlib(settings).enter(|vm| {
        let scope = vm.new_scope_with_builtins();
        match vm.run_code_string(scope.clone(), &wrapped, "<stdin>".to_owned()) {
            Ok(_) => match scope.globals.get_item("__fun_out", vm) {
                Ok(obj) => obj
                    .str(vm)
                    .map(|s| s.as_str().to_string())
                    .map_err(|e| format!("{e:?}")),
                Err(_) => Ok(String::new()),
            },
            Err(exc) => {
                let mut msg = String::new();
                let _ = vm.write_exception(&mut msg, &exc);
                Err(if msg.trim().is_empty() {
                    "python error".into()
                } else {
                    msg
                })
            }
        }
    })
}
