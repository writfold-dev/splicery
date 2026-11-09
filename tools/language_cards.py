"""Quickstart-card and admitted-syntax checks for the language gate (SG04).

The quickstart card (docs/agent-quickstart.md) keeps its recipes in fenced
``splicery`` blocks whose first line is ``# card: NAME``; each NAME has exactly
one vector in qualification/quickstart-vectors.json. The admitted-syntax
paragraph of docs/offline-preview.md must name every EBNF keyword, and every
keyword must be exercised by a grammar accept case, an example or a card.
"""
from __future__ import annotations

import json
from pathlib import Path
import re
import sys

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "tools"))
from grammar import KEYWORDS, lex, parse  # noqa: E402

FENCE = re.compile(r"^```splicery\n(.*?)^```$", re.MULTILINE | re.DOTALL)
CARD_NAME = re.compile(r"# card: ([a-z0-9-]+)\n")
HEADING = "## Admitted syntax and limits"
# Lexical terminals spelled with a word prefix (HEX is hex"…", RX is rx"…").
PREFIXED_TERMINALS = {"hex", "rx"}


def card_recipes() -> dict[str, str]:
    text = (ROOT / "docs/agent-quickstart.md").read_text(encoding="utf-8")
    cards: dict[str, str] = {}
    for block in FENCE.findall(text):
        named = CARD_NAME.match(block)
        if not named:
            raise ValueError("every quickstart recipe must start with '# card: NAME'")
        if named.group(1) in cards:
            raise ValueError(f"duplicate quickstart card {named.group(1)}")
        cards[named.group(1)] = block
    return cards


def card_vectors() -> list[dict]:
    vectors = json.loads((ROOT / "qualification/quickstart-vectors.json").read_text(encoding="utf-8"))
    names = [vector["name"] for vector in vectors]
    if len(names) != len(set(names)) or set(names) != set(card_recipes()):
        raise ValueError("quickstart cards and quickstart vectors must name the same recipes")
    return vectors


def admitted_syntax() -> dict:
    text = (ROOT / "docs/offline-preview.md").read_text(encoding="utf-8")
    if text.count(HEADING) != 1:
        raise ValueError("offline-preview.md must have exactly one admitted-syntax section")
    paragraph = text.split(HEADING, 1)[1].strip().split("\n\n", 1)[0]
    code = " ".join(re.findall(r"`([^`]+)`", paragraph))
    words = set(re.findall(r"[a-z_]+", re.sub(r"@[A-Za-z_][A-Za-z0-9_]*", " ", code)))
    missing = sorted(KEYWORDS - words)
    if missing:
        raise ValueError(f"admitted-syntax paragraph omits EBNF keywords: {missing}")
    stray = sorted(words - KEYWORDS - PREFIXED_TERMINALS)
    if stray:
        raise ValueError(f"admitted-syntax paragraph names non-grammar forms: {stray}")
    corpus = list(json.loads((ROOT / "qualification/grammar-cases.json").read_text())["accept"])
    corpus += [path.read_text(encoding="utf-8") for path in sorted((ROOT / "examples").glob("*.spl"))]
    corpus += list(card_recipes().values())
    exercised: set[str] = set()
    for source in corpus:
        parse(source)
        exercised |= {token.kind for token in lex(source)}
    unexercised = sorted(KEYWORDS - exercised)
    if unexercised:
        raise ValueError(f"EBNF keywords without an accepted grammar case: {unexercised}")
    return {"keywords": sorted(KEYWORDS), "paragraph_forms": sorted(words),
            "accepted_sources": len(corpus)}
