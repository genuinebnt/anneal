#!/usr/bin/env python3
"""Checks cookbook pages: fields, anneal's Markdown grammar, snippet provenance, and that the code builds, runs and passes."""
import pathlib, re, subprocess, sys, tempfile, textwrap, tomllib, concurrent.futures as cf

ROOT = pathlib.Path(__file__).resolve().parent
KINDS = {'concept', 'api', 'pattern', 'comparison', 'build', 'testing'}
LEVELS = {'simple', 'intermediate', 'advanced'}
SOURCE_KINDS = {'paper', 'docs', 'guide', 'book', 'standard', 'article', 'video', 'code', 'blog', 'series', 'slides'}
LANGS = {'rust', 'rs', 'python', 'py', 'sql', 'http', 'json', 'redis', 'sh', 'bash', 'text'}
CALLOUTS = {'NOTE', 'TIP', 'WARNING', 'ASIDE', 'RUST', 'TESTING', 'PRODUCTION'}
REQUIRED = {'title', 'module', 'chapter', 'order', 'kind', 'level', 'status', 'lede', 'summary', 'sections', 'sources', 'verified'}
OPTIONAL = {'layers', 'layers_on', 'glance', 'terms', 'related', 'labs', 'illustrative', 'credit', 'methods'}
SUB = {'sections': ({'heading', 'body'}, set()), 'glance': ({'label', 'value'}, set()), 'terms': ({'term', 'means', 'section'}, set()),
       'sources': ({'title', 'url', 'kind', 'note', 'read'}, {'scope'}),
       'methods': ({'type', 'name', 'signature', 'since', 'use_when'}, {'common', 'unstable', 'anchor'}), 'labs': ({'title', 'after', 'files'}, {'run', 'test'})}


def markdown(text):
    """Port of check_markdown in anneal's sd.rs, plus the new callout kinds. Returns (problems, fences)."""
    bad, fences, lines, i = [], [], text.split('\n'), 0
    while i < len(lines):
        line = lines[i]
        if line.startswith('```'):
            words = line[3:].split()
            lang, opts = (words[0] if words else 'text'), {}
            if lang not in LANGS:
                bad.append(f'unknown code language {lang!r}')
            for w in words[1:]:
                k, _, v = w.partition('=')
                if k in ('title', 'group') and v:
                    opts[k] = v
                elif k == 'hl' and re.fullmatch(r'\d+(-\d+)?(,\d+(-\d+)?)*', v):
                    opts[k] = v
                elif k == 'compile_fail' and lang in ('rust', 'rs') and re.fullmatch(r'E\d{4}', v):
                    opts[k] = v
                else:
                    bad.append(f'bad fence option {w!r}')
            start, i = i, i + 1
            while i < len(lines) and lines[i].rstrip() != '```':
                i += 1
            if i == len(lines):
                bad.append(f'code fence on body line {start + 1} is never closed')
            body = lines[start + 1:i]
            if 'hl' in opts and any(not 1 <= int(n) <= len(body) for n in re.findall(r'\d+', opts['hl'])):
                bad.append(f'hl={opts["hl"]} points outside a {len(body)}-line snippet')
            fences.append((lang, opts, '\n'.join(body)))
        elif line.rstrip() == '$$':
            i += 1
            while i < len(lines) and lines[i].rstrip() != '$$':
                i += 1
            if i == len(lines):
                bad.append('display math is never closed')
        elif line.startswith(':::'):
            kind, _, title = line[3:].partition(' ')
            if kind not in ('steps', 'deeper') or not title.strip():
                bad.append(f'bad block {line!r}')
            items, i = 0, i + 1
            while i < len(lines) and lines[i].rstrip() != ':::':
                if lines[i].startswith(':::'):
                    bad.append('blocks cannot be nested')
                if kind == 'steps' and lines[i].strip():
                    parts = lines[i].removeprefix('- ').split(' | ')
                    if not lines[i].startswith('- ') or len(parts) not in (2, 3) or not all(p.strip() for p in parts):
                        bad.append(f'bad step {lines[i]!r}')
                    items += 1
                i += 1
            if i == len(lines):
                bad.append(f':::{kind} is never closed')
            elif kind == 'steps' and items < 2:
                bad.append(':::steps needs two steps')
        else:
            m = re.match(r'> \[!(\w+)\]', line)
            if m and m[1] not in CALLOUTS:
                bad.append(f'unknown callout {m[1]}')
            if re.match(r'#{1,2} ', line):
                bad.append(f'heading {line!r}: use the section heading, or ###')
            if line.count('\\(') != line.count('\\)'):
                bad.append('inline math does not pair up')
        i += 1
    return bad, fences


def squash(code):
    return '\n'.join(l.rstrip() for l in textwrap.dedent(code).strip('\n').split('\n'))


def excerpt_of(snippet, source):
    """True if the snippet is a contiguous run of source lines, ignoring a common indentation."""
    want = squash(snippet).split('\n')
    have = [l.rstrip() for l in source.split('\n')]
    for start in range(len(have) - len(want) + 1):
        if squash('\n'.join(have[start:start + len(want)])).split('\n') == want:
            return True
    return False


def run(cmd, cwd):
    return subprocess.run(cmd, cwd=cwd, capture_output=True, text=True, timeout=900, stdin=subprocess.DEVNULL)


def compile_error_codes(snippet):
    """Compiles a std-only snippet on its own; returns (compiled, set of error codes)."""
    source = '#![allow(unused)]\n' + snippet + ('\n' if re.search(r'\bfn main\b', snippet) else '\nfn main() {}\n')
    with tempfile.TemporaryDirectory() as tmp:
        f = pathlib.Path(tmp) / 'snippet.rs'
        f.write_text(source)
        r = run(['rustc', '--edition', '2024', '--crate-name', 'snippet', '--emit=metadata', '-o', str(f.with_suffix('.rmeta')), str(f)], tmp)
    return r.returncode == 0, set(re.findall(r'error\[(E\d{4})\]', r.stderr))


def std_function_names():
    """Every `fn name` declared in the installed core, alloc and std sources."""
    sysroot = run(['rustc', '--print', 'sysroot'], ROOT).stdout.strip()
    lib = pathlib.Path(sysroot) / 'lib/rustlib/src/rust/library'
    names = set()
    for crate in ('core', 'alloc', 'std'):
        for f in (lib / crate / 'src').rglob('*.rs'):
            names.update(re.findall(r'\bfn\s+([A-Za-z_]\w*)', f.read_text(errors='replace')))
    return names


def check(page, links, strict=False):
    errors, warnings = [], []
    d = ROOT / page
    try:
        t = tomllib.loads((d / 'page.toml').read_text())
    except Exception as e:
        return [f'page.toml: {e}'], []
    errors += [f'missing field {k}' for k in sorted(REQUIRED - t.keys())]
    errors += [f'unknown field {k}' for k in sorted(t.keys() - REQUIRED - OPTIONAL)]
    for key, (need, may) in SUB.items():
        for n, item in enumerate(t.get(key, [])):
            errors += [f'{key}[{n}] missing {k}' for k in sorted(need - item.keys())]
            errors += [f'{key}[{n}] unknown {k}' for k in sorted(item.keys() - need - may)]
    if errors:
        return errors, warnings
    if t['kind'] not in KINDS: errors.append('bad kind')
    if t['level'] not in LEVELS: errors.append('bad level')
    if t['status'] != 'draft': errors.append('status must be draft until the owner reviews the page')
    if not 3 <= len(t['summary']) <= 7: errors.append('summary needs 3 to 7 sentences')
    if len(t['sources']) < 3: errors.append('at least 3 sources')
    errors += [f'source kind {s["kind"]!r}' for s in t['sources'] if s['kind'] not in SOURCE_KINDS]
    if not any(s['read'] for s in t['sources']): errors.append('no source is marked read = true')
    errors += [f'[verified] missing {k}' for k in ('toolchain', 'date', 'platform', 'not_checked') if k not in t['verified']]
    headings = [s['heading'] for s in t['sections']]
    if len(set(headings)) != len(headings): errors.append('duplicate section heading')
    errors += [f'term {x["term"]!r}: no section {x["section"]!r}' for x in t.get('terms', []) if x['section'] not in headings]
    errors += [f'layers_on {x!r} is not in layers' for x in t.get('layers_on', []) if x not in t.get('layers', [])]
    errors += [f'lab {x["title"]!r}: no section {x["after"]!r}' for x in t.get('labs', []) if x['after'] not in headings]
    warnings += [f'related page {r!r} does not exist yet' for r in t.get('related', []) if not (ROOT / r / 'page.toml').exists()]
    code = d / 'code'
    sources = {str(p.relative_to(code)): p.read_text() for p in code.rglob('*.rs') if 'target' not in p.relative_to(code).parts} if code.exists() else {}
    illustrative, used = set(t.get('illustrative', [])), set()
    anchors = [a for s in t['sections'] for a in re.findall(r'^### .*\{#([\w-]+)\}\s*$', s['body'], re.M)]
    if len(set(anchors)) != len(anchors): errors.append('duplicate ### {#id}')
    for s in t['sections']:
        bad, fences = markdown(s['body'])
        errors += [f'section {s["heading"]!r}: {b}' for b in bad]
        for lang, opts, snippet in fences:
            if lang in ('rust', 'rs'):
                title = opts.get('title', '')
                named = [src for path, src in sources.items() if path == title]
                if 'compile_fail' in opts:
                    compiled, codes = compile_error_codes(snippet)
                    if compiled: errors.append(f'section {s["heading"]!r}: snippet marked compile_fail={opts["compile_fail"]} compiles')
                    elif opts['compile_fail'] not in codes: errors.append(f'section {s["heading"]!r}: expected {opts["compile_fail"]}, got {sorted(codes) or "another error"}')
                elif title in illustrative:
                    used.add(title)
                    warnings.append(f'unchecked snippet {title!r}')
                elif not any(excerpt_of(snippet, src) for src in (named or sources.values())):
                    errors.append(f'section {s["heading"]!r}: rust snippet {title or snippet.splitlines()[:1]} is not an excerpt of any file in code/')
    errors += [f'illustrative title {x!r} matches no fence' for x in sorted(illustrative - used)]
    if t.get('methods'):
        known = std_function_names()
        for m in t['methods']:
            if m['name'] not in known: warnings.append(f'method {m["type"]}::{m["name"]} is not declared in core, alloc or std (fine for a third-party crate)')
            if not re.search(r'\bfn\s+' + re.escape(m['name']) + r'\b', m['signature']): errors.append(f'method {m["name"]!r}: signature does not declare fn {m["name"]}')
            if m.get('anchor') and m['anchor'] not in anchors and m['anchor'] not in headings: errors.append(f'method {m["name"]!r}: anchor {m["anchor"]!r} is not a heading or ### id')
    for lab in t.get('labs', []):
        errors += [f'lab file code/{f} does not exist' for f in lab['files'] if f not in sources]
    if sources:
        manifest = code / 'Cargo.toml'
        if not manifest.exists() or '[workspace]' not in manifest.read_text():
            errors.append('code/Cargo.toml is missing or has no [workspace] table')
        else:
            target = ['--target-dir', str(ROOT.parent / 'target' / 'pages')]
            b = run(['cargo', 'build', '--release', '--all-targets', *target], code)
            if b.returncode: errors.append('code does not build:\n' + b.stderr[-1500:])
            else:
                if re.search(r'^warning', b.stderr, re.M): errors.append('code builds with warnings')
                r = run(['cargo', 'test', '--release', *target], code)
                if r.returncode: errors.append('tests fail:\n' + (r.stdout + r.stderr)[-1500:])
                if strict:
                    for label, cmd in (('rustfmt', ['cargo', 'fmt', '--check']), ('clippy', ['cargo', 'clippy', '--all-targets', *target, '--', '-D', 'warnings']),
                                       ('debug tests', ['cargo', 'test', *target])):
                        r = run(cmd, code)
                        if r.returncode: errors.append(f'{label} fails:\n' + (r.stdout + r.stderr)[-1200:])
                if any(l.get('run') for l in t.get('labs', [])):
                    r = run(['cargo', 'run', '--release', '-q', *target], code)
                    if r.returncode: errors.append('cargo run fails:\n' + r.stderr[-1500:])
    if links:
        def status(u):
            return u, run(['curl', '-s', '-o', '/dev/null', '-L', '--max-time', '25', '-w', '%{http_code}', '--', u], d).stdout
        with cf.ThreadPoolExecutor(6) as ex:
            for u, c in ex.map(status, [s['url'] for s in t['sources']]):
                if c in ('404', '410'): errors.append(f'broken link {u}')
                elif c != '200': warnings.append(f'unconfirmed link ({c}) {u}')
    return errors, warnings


if __name__ == '__main__':
    args = [a for a in sys.argv[1:] if not a.startswith('--')]
    pages = args or sorted(p.parent.name for p in ROOT.glob('*/page.toml'))
    failed = 0
    for page in pages:
        errors, warnings = check(page, '--links' in sys.argv, '--strict' in sys.argv)
        print(f'{"FAIL" if errors else "ok  "} {page}')
        for e in errors: print('   error:', e)
        for w in warnings: print('   warn: ', w)
        failed += bool(errors)
    print(f'{len(pages)} pages, {failed} failing')
    sys.exit(1 if failed else 0)
