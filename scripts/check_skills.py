"""Writes every Rust snippet of the user skills into examples/zz_check.rs, so
`cargo build --example zz_check` checks they compile. Delete the file afterwards.

Each code block goes into its own module. A skill's first block, when it holds only `use`
lines, is the imports every other block of that skill gets.
"""
import glob, re

out = ['#![allow(unused)]', 'fn main() {}']
for path in sorted(glob.glob('plugins/iso-ui/skills/*/SKILL.md')):
    blocks = re.findall(r'```rust\n(.*?)```', open(path).read(), re.S)
    header = ''
    if blocks and all(line.startswith('use ') or not line.strip() for line in blocks[0].splitlines()):
        header = blocks.pop(0)
    name = path.split('/')[-2].replace('-', '_')
    for i, block in enumerate(blocks):
        # the layout skill's setup loads a font the examples have
        block = block.replace('include_bytes!("../assets/Font.ttf")', 'include_bytes!("./LiberationSans-Regular.ttf")')
        out.append(f'mod {name}_{i} {{\n{header}\n{block}\n}}')
open('examples/zz_check.rs', 'w').write('\n'.join(out) + '\n')
print(len(out) - 2, 'snippets written to examples/zz_check.rs')
