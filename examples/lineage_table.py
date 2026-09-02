"""A table of taxonomic lineages, for `phylo_tree.py` to draw.

Returns a real `Table`: a transform's return value is coerced the same way any
script value is, and anything carrying Arrow columns — a `pyarrow.Table`
included — becomes one. The column is the format every metagenomics tool
speaks, semicolons between ranks and a one-letter prefix on each:

    d__Bacteria;p__Bacillota;c__Bacilli;o__Lactobacillales;...;s__Streptococcus pneumoniae

**This example needs `pyarrow`.** There is no way to build Arrow columns from
the standard library, and the interpreter dex embeds may not have it. Point the
Settings tab's global environment at one that does, and this runs. Nothing else
here needs a package: `phylo_tree.py` parses these strings with `str.split`.
"""

import random

# The ranks a lineage carries, in order, with the prefix each is written with.
RANKS = [
    ("d", "Domain"),
    ("p", "Phylum"),
    ("c", "Class"),
    ("o", "Order"),
    ("f", "Family"),
    ("g", "Genus"),
    ("s", "Species"),
]


def _lineage(*names):
    """A lineage string from seven names, outermost first."""
    return ";".join(f"{prefix}__{name}" for (prefix, _), name in zip(RANKS, names))


# A small, real-shaped sample: a few clades deep enough to branch, so the tree
# has something to show. Read counts are made up.
SAMPLE = [
    ("Bacteria", "Bacillota", "Bacilli", "Lactobacillales", "Streptococcaceae",
     "Streptococcus", "Streptococcus pneumoniae"),
    ("Bacteria", "Bacillota", "Bacilli", "Lactobacillales", "Streptococcaceae",
     "Streptococcus", "Streptococcus pyogenes"),
    ("Bacteria", "Bacillota", "Bacilli", "Lactobacillales", "Lactobacillaceae",
     "Lactobacillus", "Lactobacillus acidophilus"),
    ("Bacteria", "Bacillota", "Bacilli", "Bacillales", "Staphylococcaceae",
     "Staphylococcus", "Staphylococcus aureus"),
    ("Bacteria", "Bacillota", "Bacilli", "Bacillales", "Bacillaceae",
     "Bacillus", "Bacillus subtilis"),
    ("Bacteria", "Bacillota", "Clostridia", "Eubacteriales", "Clostridiaceae",
     "Clostridium", "Clostridium botulinum"),
    ("Bacteria", "Pseudomonadota", "Gammaproteobacteria", "Enterobacterales",
     "Enterobacteriaceae", "Escherichia", "Escherichia coli"),
    ("Bacteria", "Pseudomonadota", "Gammaproteobacteria", "Enterobacterales",
     "Enterobacteriaceae", "Salmonella", "Salmonella enterica"),
    ("Bacteria", "Pseudomonadota", "Gammaproteobacteria", "Enterobacterales",
     "Yersiniaceae", "Yersinia", "Yersinia pestis"),
    ("Bacteria", "Pseudomonadota", "Gammaproteobacteria", "Pseudomonadales",
     "Pseudomonadaceae", "Pseudomonas", "Pseudomonas aeruginosa"),
    ("Bacteria", "Pseudomonadota", "Alphaproteobacteria", "Rhizobiales",
     "Rhizobiaceae", "Agrobacterium", "Agrobacterium tumefaciens"),
    ("Bacteria", "Actinomycetota", "Actinomycetes", "Mycobacteriales",
     "Mycobacteriaceae", "Mycobacterium", "Mycobacterium tuberculosis"),
    ("Bacteria", "Actinomycetota", "Actinomycetes", "Mycobacteriales",
     "Corynebacteriaceae", "Corynebacterium", "Corynebacterium diphtheriae"),
    ("Bacteria", "Bacteroidota", "Bacteroidia", "Bacteroidales",
     "Bacteroidaceae", "Bacteroides", "Bacteroides fragilis"),
    ("Archaea", "Euryarchaeota", "Methanobacteria", "Methanobacteriales",
     "Methanobacteriaceae", "Methanobrevibacter", "Methanobrevibacter smithii"),
    ("Archaea", "Euryarchaeota", "Halobacteria", "Halobacteriales",
     "Halobacteriaceae", "Halobacterium", "Halobacterium salinarum"),
]


def lineages():
    """The sample lineages, as the strings a tool would emit."""
    return [_lineage(*names) for names in SAMPLE]


def rows(seed=11):
    """`(lineage, reads, abundance)` per taxon, with plausible made-up counts."""
    rng = random.Random(seed)
    counts = [rng.randint(120, 90_000) for _ in SAMPLE]
    total = sum(counts)
    return [
        (lineage, count, round(100.0 * count / total, 4))
        for lineage, count in zip(lineages(), counts)
    ]


def transform():
    """A table of lineages and their read counts."""
    import pyarrow

    lineage, reads, abundance = zip(*rows())
    return pyarrow.table(
        {
            "lineage": list(lineage),
            "reads": list(reads),
            "abundance": list(abundance),
        }
    )
