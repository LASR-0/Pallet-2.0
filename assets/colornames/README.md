# Colour name dataset

`colornames.csv` is vendored from [meodai/color-names](https://github.com/meodai/color-names),
copyright (c) 2017 David Aerne, MIT licensed. The full licence text is in `LICENSE`.

## Format

`name,hex,good name` — 31,916 rows. The third column carries `x` on the ~4,959
names in upstream's curated subset. Pallet uses it only to break ties between
perceptually identical candidates.

Be aware the flag marks curation, not familiarity: `Crisps` and `Fabric of Love`
carry it while `Carrot` and `Mandarin` do not outrank them. This is a dataset of
paint and marketing names, so a close match is often evocative rather than
plain — the same register as the prototype's own `Rob Roy` and `Karry`, but it
does mean lookups can surface obscure names like `Carroburg Crimson`.

## Updating

    curl -sL https://raw.githubusercontent.com/meodai/color-names/main/src/colornames.csv \
      -o assets/colornames/colornames.csv

The parser in `crates/pallet-color/src/naming.rs` validates the header on load,
so a format change upstream fails loudly at the first lookup rather than
silently mis-parsing.
