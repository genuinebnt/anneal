import tomllib, pathlib, re, subprocess
lib = pathlib.Path(subprocess.check_output(['rustc','--print','sysroot'], text=True).strip()) / 'lib/rustlib/src/rust/library'

def norm(s):
    s = re.sub(r',\s*A(?=>)', '', s)                 # allocator parameter
    s = re.sub(r'\s+', ' ', s).strip()
    s = s.replace('( ', '(').replace(', )', ')').replace(',)', ')').replace(' )', ')')
    return s.rstrip(' ,')

def blocks(path, header):
    """Methods in the impl block whose first line matches `header`: name -> (signature, since or unstable)."""
    lines = path.read_text().split('\n'); out = {}
    for i, l in enumerate(lines):
        if re.match(header, l):
            attrs, j = [], i + 1
            while lines[j] != '}':
                t = lines[j].strip()
                if t.startswith('#['): attrs.append(t)
                if re.match(r'    pub (const )?fn ', lines[j]):
                    sig = []
                    while '{' not in lines[j]:
                        sig.append(lines[j]); j += 1
                    sig.append(lines[j].split('{')[0])
                    sig = norm(' '.join(sig))
                    name = re.search(r'fn (\w+)', sig)[1]
                    a = ' '.join(attrs)
                    m = re.search(r'\bstable\(feature = "\w+", since = "([\d.]+)"', a)
                    u = re.search(r'unstable\(feature = "(\w+)"', a)
                    out[name] = (sig, f'unstable ({u[1]})' if u else m[1])
                    attrs = []
                j += 1
    return out

m = lib / 'std/src/collections/hash/map.rs'
src = {
    'Entry': {**blocks(m, r"impl<'a, K, V, A: Allocator> Entry<"), **blocks(m, r"impl<'a, K, V: Default> Entry<")},
    'OccupiedEntry': blocks(m, r"impl<'a, K, V, A: Allocator> OccupiedEntry<"),
    'VacantEntry': blocks(m, r"impl<'a, K: 'a, V: 'a, A: Allocator> VacantEntry<"),
    'Cow': blocks(lib / 'alloc/src/borrow.rs', r"impl<B: \?Sized \+ ToOwned> Cow<'_, B> \{"),
}
for slug in ('hashmap-entry-api', 'cow-borrow-until-you-must-own'):
    t = tomllib.loads(pathlib.Path(slug, 'page.toml').read_text())
    listed, problems = {}, 0
    for x in t['methods']:
        if x['type'] == 'HashMap': continue
        listed.setdefault(x['type'], set()).add(x['name'])
        sig, since = src[x['type']][x['name']]
        have = norm(x['signature'])
        # `where` clauses that live on the impl block are written on the method in the page
        if x['name'] == 'or_default': have = have.replace(' where V: Default', '')
        if have != sig or x['since'] != since:
            problems += 1; print('DIFF', x['type'], x['name'], '\n  page  ', have, x['since'], '\n  source', sig, since)
    for ty, names in listed.items():
        if set(src[ty]) != names: problems += 1; print('MISSING', ty, set(src[ty]) ^ names)
    print(slug, 'methods:', len(t['methods']), 'common:', sum(x.get('common', False) for x in t['methods']), 'problems:', problems)
