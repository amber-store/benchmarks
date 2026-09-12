#!/usr/bin/env python3
"""List the exported API of a Rust core checkout, one symbol per line.

Output lines are `module::symbol` and `module::Type::method`, in sorted
order, so the coverage matrix can be checked against the surface it claims to
cover rather than against memory.

Publicity is decided syntactically: `pub` counts, `pub(crate)`, `pub(super)`
and `pub(in ...)` do not, and `#[cfg(test)]` modules are skipped. A module
directory (`fstree/`) is one namespace: its files' `pub` items are reachable
through the module's `pub use` re-exports, and the private submodule paths
themselves are not API.
"""
import os
import re
import sys

PUB = re.compile(r'^\s*pub\s+(?:unsafe\s+)?(?:const\s+)?(fn|struct|enum|trait|const|type|static)\s+([A-Za-z_][A-Za-z0-9_]*)')
IMPL = re.compile(r'^impl(?:<[^>]*>)?\s+(?:([A-Za-z_][A-Za-z0-9_:<>, ]*?)\s+for\s+)?([A-Za-z_][A-Za-z0-9_]*)')
MOD_DECL = re.compile(r'^\s*pub\s+mod\s+([A-Za-z_][A-Za-z0-9_]*)\s*;', re.M)
SKIP_FILES = {'tests.rs', 'testutil.rs'}


def module_files(src):
    """Maps each public module name to the files that make it up."""
    lib = open(os.path.join(src, 'lib.rs')).read()
    mods = MOD_DECL.findall(lib)
    out = {}
    for m in mods:
        flat = os.path.join(src, f'{m}.rs')
        if os.path.isfile(flat):
            out[m] = [flat]
            continue
        d = os.path.join(src, m)
        files = []
        for name in sorted(os.listdir(d)):
            if not name.endswith('.rs') or name in SKIP_FILES or name.endswith('_tests.rs'):
                continue
            files.append(os.path.join(d, name))
        out[m] = files
    return out


def scan(path):
    """Exported items of one file: (free items, {type: [methods]})."""
    items, methods = [], {}
    depth = 0
    impl_stack = []          # (brace depth at entry, type name, is_trait_impl)
    in_test_mod = None
    prev_attr_cfg_test = False
    for raw in open(path):
        line = raw.rstrip('\n')
        stripped = line.strip()

        if in_test_mod is not None:
            depth += line.count('{') - line.count('}')
            if depth <= in_test_mod:
                in_test_mod = None
            continue

        if stripped.startswith('#[cfg(test)]'):
            prev_attr_cfg_test = True
            continue

        if prev_attr_cfg_test:
            prev_attr_cfg_test = False
            if re.match(r'^\s*(pub\s+)?mod\s+', stripped):
                in_test_mod = depth
                depth += line.count('{') - line.count('}')
                continue

        m = IMPL.match(line)
        if m and not stripped.startswith('//'):
            trait_name, type_name = m.group(1), m.group(2)
            impl_stack.append((depth, type_name, trait_name is not None))
        else:
            pm = PUB.match(line)
            if pm and not stripped.startswith('//'):
                kind, name = pm.group(1), pm.group(2)
                cur = None
                for entry in reversed(impl_stack):
                    cur = entry
                    break
                if cur and not cur[2] and kind == 'fn':
                    methods.setdefault(cur[1], []).append(name)
                elif cur and cur[2]:
                    pass  # trait impls are the trait's API, not a new symbol
                else:
                    items.append((kind, name))

        depth += line.count('{') - line.count('}')
        while impl_stack and depth <= impl_stack[-1][0]:
            impl_stack.pop()
    return items, methods


def main():
    if len(sys.argv) != 2:
        print('usage: rustexports.py <core-rs-source-dir>', file=sys.stderr)
        raise SystemExit(2)
    src = os.path.join(sys.argv[1], 'src')
    out = []
    for mod, files in module_files(src).items():
        for path in files:
            items, methods = scan(path)
            for _kind, name in items:
                out.append(f'{mod}::{name}')
            for ty, ms in methods.items():
                for name in ms:
                    out.append(f'{mod}::{ty}::{name}')
    for s in sorted(set(out)):
        print(s)


if __name__ == '__main__':
    main()
