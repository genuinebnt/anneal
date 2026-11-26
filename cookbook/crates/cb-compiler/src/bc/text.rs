//! Disassembler and assembler. The text format round-trips:
//! `assemble(&disassemble(&chunk))` rebuilds an equivalent chunk (same code,
//! scopes and debug info) whose disassembly is identical.
//!
//! ```text
//! main f0
//! const k0 = int 5000000000
//! const k1 = str "hi"
//! func f0 "main" regs=3 params=0 const=false
//!   scope 0 "main" -
//!   0: loadi r0, 5        ; s0
//!   1: ret r0             ; s0
//! end
//! ```

use super::*;
use std::fmt::Write;

pub fn disassemble(c: &Chunk) -> String {
    let mut s = String::new();
    let _ = writeln!(s, "main f{}", c.main);
    for (i, k) in c.consts.iter().enumerate() {
        match k {
            BConst::Int(v) => writeln!(s, "const k{i} = int {v}"),
            BConst::Str(t) => writeln!(s, "const k{i} = str {:?}", &**t),
        }
        .ok();
    }
    for (i, f) in c.funcs.iter().enumerate() {
        disassemble_func(&mut s, i, f);
    }
    s
}

fn disassemble_func(s: &mut String, i: usize, f: &BcFunc) {
    let _ = writeln!(
        s,
        "func f{i} {:?} regs={} params={} const={}",
        f.name, f.nregs, f.nparams, f.is_const
    );
    for (si, sc) in f.scopes.iter().enumerate() {
        let parent = sc.parent.map_or("-".to_string(), |p| p.to_string());
        let _ = writeln!(s, "  scope {si} {:?} {parent}", sc.name);
    }
    for (pc, ins) in f.code.iter().enumerate() {
        let ops = ins.operands(f).join(", ");
        let line = if ops.is_empty() {
            ins.mnemonic().to_string()
        } else {
            format!("{} {ops}", ins.mnemonic())
        };
        let _ = writeln!(
            s,
            "  {pc}: {line:<28} ; s{}",
            f.pc_scope.get(pc).copied().unwrap_or(0)
        );
    }
    let _ = writeln!(s, "end");
}

/// Split `a, [r1, r2], {1:@2, _:@3}` at top-level commas.
fn split_operands(s: &str) -> Vec<&str> {
    let (mut out, mut depth, mut start) = (Vec::new(), 0i32, 0);
    for (i, ch) in s.char_indices() {
        match ch {
            '[' | '{' => depth += 1,
            ']' | '}' => depth -= 1,
            ',' if depth == 0 => {
                out.push(s[start..i].trim());
                start = i + 1;
            }
            _ => {}
        }
    }
    let last = s[start..].trim();
    if !last.is_empty() {
        out.push(last);
    }
    out
}

/// Parse a Rust-style quoted string at the start of `s`; returns the value and the rest.
fn unquote(s: &str) -> Option<(String, &str)> {
    let mut chars = s.strip_prefix('"')?.char_indices();
    let mut out = String::new();
    let body = &s[1..];
    while let Some((i, c)) = chars.next() {
        match c {
            '"' => return Some((out, &body[i + 1..])),
            '\\' => match chars.next()?.1 {
                'n' => out.push('\n'),
                't' => out.push('\t'),
                'r' => out.push('\r'),
                '0' => out.push('\0'),
                '\\' => out.push('\\'),
                '"' => out.push('"'),
                '\'' => out.push('\''),
                'u' => {
                    let mut hex = String::new();
                    chars.next()?; // '{'
                    for (_, h) in chars.by_ref() {
                        if h == '}' {
                            break;
                        }
                        hex.push(h);
                    }
                    out.push(char::from_u32(u32::from_str_radix(&hex, 16).ok()?)?);
                }
                _ => return None,
            },
            c => out.push(c),
        }
    }
    None
}

pub fn assemble(text: &str) -> Result<Chunk, String> {
    let mut chunk = Chunk {
        funcs: vec![],
        consts: vec![],
        main: 0,
    };
    let mut cur: Option<BcFunc> = None;
    for (ln, raw) in text.lines().enumerate() {
        let err = |m: &str| format!("line {}: {m}: `{}`", ln + 1, raw.trim());
        let line = raw.trim();
        if line.is_empty() {
            continue;
        }
        if let Some(f) = cur.as_mut() {
            if line == "end" {
                chunk.funcs.push(cur.take().expect("current function"));
            } else if let Some(rest) = line.strip_prefix("scope ") {
                let (idx, rest) = rest.split_once(' ').ok_or_else(|| err("bad scope"))?;
                let (name, rest) = unquote(rest.trim()).ok_or_else(|| err("bad scope name"))?;
                let parent = match rest.trim() {
                    "-" => None,
                    p => Some(p.parse().map_err(|_| err("bad scope parent"))?),
                };
                let idx: usize = idx.parse().map_err(|_| err("bad scope index"))?;
                if idx == 0 {
                    f.scopes.clear();
                }
                if idx != f.scopes.len() {
                    return Err(err("scopes must be listed in order"));
                }
                f.scopes.push(Scope {
                    name,
                    parent,
                    call_site: Default::default(),
                });
            } else {
                let (code, scope) = match line.split_once(';') {
                    Some((c, s)) => (
                        c.trim(),
                        s.trim()
                            .strip_prefix('s')
                            .and_then(|x| x.parse().ok())
                            .unwrap_or(0),
                    ),
                    None => (line, 0),
                };
                let code = match code.split_once(':') {
                    Some((pc, rest)) if pc.trim().parse::<usize>().is_ok() => rest.trim(),
                    _ => code,
                };
                let (mn, rest) = code.split_once(' ').unwrap_or((code, ""));
                let toks = split_operands(rest);
                let mut b = Builder {
                    lists: &mut f.lists,
                    switches: &mut f.switches,
                };
                let ins = Instr::parse(mn, &toks, &mut b).ok_or_else(|| err("bad instruction"))?;
                f.code.push(ins);
                f.pc_scope.push(scope);
            }
        } else if let Some(rest) = line.strip_prefix("main f") {
            chunk.main = rest.parse().map_err(|_| err("bad main"))?;
        } else if let Some(rest) = line.strip_prefix("const k") {
            let (idx, rest) = rest.split_once(" = ").ok_or_else(|| err("bad const"))?;
            if idx.parse::<usize>().ok() != Some(chunk.consts.len()) {
                return Err(err("constants must be listed in order"));
            }
            let k = if let Some(v) = rest.strip_prefix("int ") {
                BConst::Int(v.trim().parse().map_err(|_| err("bad int const"))?)
            } else if let Some(v) = rest.strip_prefix("str ") {
                BConst::Str(
                    unquote(v.trim())
                        .ok_or_else(|| err("bad string const"))?
                        .0
                        .into(),
                )
            } else {
                return Err(err("unknown const kind"));
            };
            chunk.consts.push(k);
        } else if let Some(rest) = line.strip_prefix("func f") {
            let (idx, rest) = rest.split_once(' ').ok_or_else(|| err("bad func"))?;
            if idx.parse::<usize>().ok() != Some(chunk.funcs.len()) {
                return Err(err("functions must be listed in order"));
            }
            let (name, rest) = unquote(rest.trim()).ok_or_else(|| err("bad function name"))?;
            let mut f = BcFunc::empty(&name);
            for kv in rest.split_whitespace() {
                let (k, v) = kv
                    .split_once('=')
                    .ok_or_else(|| err("bad function attribute"))?;
                match k {
                    "regs" => f.nregs = v.parse().map_err(|_| err("bad regs"))?,
                    "params" => f.nparams = v.parse().map_err(|_| err("bad params"))?,
                    "const" => f.is_const = v == "true",
                    _ => return Err(err("unknown function attribute")),
                }
            }
            cur = Some(f);
        } else {
            return Err(err("unexpected line"));
        }
    }
    if cur.is_some() {
        return Err("unterminated function (missing `end`)".into());
    }
    Ok(chunk)
}
