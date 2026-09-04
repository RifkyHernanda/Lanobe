#!/usr/bin/env python3
"""
Generates .claude/ui-layout.excalidraw.

Written as a generator rather than hand-authored JSON so the diagram can be
regenerated after a layout change instead of being nudged element by element in
the editor. Excalidraw will happily open and edit the output either way.

    python3 scripts/gen-ui-diagram.py
"""
import json
import random
from pathlib import Path

random.seed(7)  # stable ids and seeds => clean diffs between regenerations

INK = "#1e1e1e"
MUTED = "#868e96"
BLUE = "#1971c2"
BLUE_BG = "#a5d8ff"
GREEN = "#2f9e44"
GREEN_BG = "#b2f2bb"
AMBER = "#e8590c"
AMBER_BG = "#ffec99"
GREY_BG = "#e9ecef"

elements = []
_seq = [0]


def _id():
    _seq[0] += 1
    return f"el{_seq[0]:04d}"


def _base(kind, x, y, w, h, **kw):
    return {
        "id": _id(),
        "type": kind,
        "x": x,
        "y": y,
        "width": w,
        "height": h,
        "angle": 0,
        "strokeColor": kw.get("stroke", INK),
        "backgroundColor": kw.get("bg", "transparent"),
        "fillStyle": "solid",
        "strokeWidth": kw.get("strokeWidth", 1),
        "strokeStyle": kw.get("strokeStyle", "solid"),
        "roughness": 1,
        "opacity": 100,
        "groupIds": [],
        "frameId": None,
        "roundness": {"type": 3} if kind == "rectangle" else None,
        "seed": random.randint(1, 2_000_000),
        "version": 1,
        "versionNonce": random.randint(1, 2_000_000),
        "isDeleted": False,
        "boundElements": [],
        "updated": 1,
        "link": None,
        "locked": False,
    }


def box(x, y, w, h, **kw):
    elements.append(_base("rectangle", x, y, w, h, **kw))


def text(x, y, s, size=16, color=INK, align="left", width=None):
    lines = s.split("\n")
    w = width or int(max(len(ln) for ln in lines) * size * 0.58) + 4
    h = int(len(lines) * size * 1.25)
    el = _base("text", x, y, w, h, stroke=color)
    el.update({
        "text": s,
        "fontSize": size,
        "fontFamily": 2,          # Helvetica - legible at small sizes
        "textAlign": align,
        "verticalAlign": "top",
        "containerId": None,
        "originalText": s,
        "lineHeight": 1.25,
        "autoResize": True,
    })
    elements.append(el)


def arrow(x1, y1, x2, y2, color=MUTED, dashed=False):
    el = _base("arrow", x1, y1, x2 - x1, y2 - y1, stroke=color,
               strokeStyle="dashed" if dashed else "solid")
    el.update({
        "points": [[0, 0], [x2 - x1, y2 - y1]],
        "lastCommittedPoint": None,
        "startBinding": None,
        "endBinding": None,
        "startArrowhead": None,
        "endArrowhead": "arrow",
        "elbowed": False,
    })
    elements.append(el)


def panel(x, y, w, h, title, subtitle=""):
    box(x, y, w, h, stroke=MUTED, strokeStyle="dashed")
    text(x + 16, y + 14, title, size=20)
    if subtitle:
        text(x + 16, y + 40, subtitle, size=12, color=MUTED)


def card(x, y, w, h, title, body="", accent=None, accent_bg=None, note=""):
    box(x, y, w, h, stroke=accent or INK, bg=accent_bg or "transparent", strokeWidth=2)
    text(x + 12, y + 12, title, size=15, color=accent or INK)
    if body:
        text(x + 12, y + 36, body, size=12, color=INK)
    if note:
        text(x + 12, y + h - 24, note, size=11, color=MUTED)


# ── Panel 1: navigation ─────────────────────────────────────────────────────
panel(0, 0, 900, 560, "1. Navigation", "Sidebar on desktop, bottom bar on mobile. Reader is full-screen, no chrome.")

box(30, 80, 190, 440, stroke=MUTED, bg=GREY_BG)
text(46, 94, "Nav bar", size=13, color=MUTED)
nav = [
    ("Novels", "/ln", "library grid"),
    ("Saved", "/saved", "kanji + words  [P3]"),
    ("Dictionary", "/dictionary", "manual search"),
    ("Settings", "/settings", "appearance"),
    ("About", "/about", ""),
    ("More", "/more", "mobile overflow"),
]
for i, (label, route, hint) in enumerate(nav):
    y = 126 + i * 62
    box(46, y, 158, 48, stroke=BLUE, bg=BLUE_BG if i == 0 else "transparent")
    text(58, y + 8, label, size=14, color=BLUE)
    text(58, y + 27, route + ("   " + hint if hint else ""), size=10, color=MUTED)

card(260, 110, 280, 150, "Library  /ln", "Cover grid, reading status,\nprogress bar, categories,\nImport EPUB.", GREEN, None)
card(260, 290, 280, 150, "Reader  /ln/:id/read", "Full screen. Tategaki or\nhorizontal, paged or scroll,\nfurigana, click zones.", GREEN, None,
     "Tap a word -> lookup popup")
card(580, 110, 280, 150, "Saved  /saved   [P3]", "Tabs: Kanji | Words.\nSearch, filter by book,\nmark known, delete.", AMBER, None,
     "Not built yet")
card(580, 290, 280, 150, "Dictionary  /dictionary", "Manual search box.\nGear -> dictionary manager\n(install / import).", GREEN, None)

arrow(224, 150, 256, 150)
arrow(400, 262, 400, 286)
arrow(224, 212, 576, 175)
arrow(224, 274, 576, 340)

# ── Panel 2: reader wireframe ───────────────────────────────────────────────
panel(960, 0, 720, 560, "2. Reader layout", "Vertical Japanese text, right to left. Chrome hides while reading.")

box(1000, 90, 640, 420, stroke=INK, strokeWidth=2)
box(1000, 90, 640, 44, stroke=MUTED, bg=GREY_BG)
text(1016, 104, "Title           progress 34%              [Aa] [⚙]", size=13, color=MUTED)

box(1030, 160, 460, 300, stroke=MUTED, strokeStyle="dashed")
text(1046, 172, "text column  (writing-mode: vertical-rl)", size=11, color=MUTED)
for i in range(7):
    x = 1450 - i * 62
    box(x, 200, 8, 230, stroke=INK if i not in (2, 4) else BLUE,
        bg="transparent" if i not in (2, 4) else BLUE_BG)
text(1046, 470, "saved kanji highlighted inline, every book", size=11, color=BLUE)

box(1510, 160, 110, 300, stroke=MUTED, strokeStyle="dashed")
text(1522, 172, "click\nzone", size=11, color=MUTED)
box(1000, 160, 20, 300, stroke=MUTED, strokeStyle="dashed")

card(1030, 500, 0, 0, "", "")
box(1180, 250, 300, 170, stroke=AMBER, bg=AMBER_BG, strokeWidth=2)
text(1196, 262, "lookup popup", size=14, color=AMBER)
text(1196, 286, "term  ·  reading  ·  pitch\nglossary from every dictionary\n[ ▶ audio ]   [ ☆ save ]", size=12)
text(1196, 392, "☆ save -> stores term + each kanji", size=11, color=MUTED)

# ── Panel 3: the core loop ──────────────────────────────────────────────────
panel(0, 620, 1680, 420, "3. Save → highlight → export", "The loop the whole project exists for. P3 and P4.")

steps = [
    ("1. Tap", "Unknown word in\nthe reader.", GREEN, None),
    ("2. Look up", "Cached or prefetched,\nso the popup is instant.", GREEN, None),
    ("3. Save", "One term row +\none row per kanji.", AMBER, AMBER_BG),
    ("4. Index", "highlight-index\n(ETag cached).", AMBER, AMBER_BG),
    ("5. Highlight", "Every book, every\noccurrence, forever.", AMBER, AMBER_BG),
    ("6. Export", "Finish book → .apkg\nwith audio.", BLUE, BLUE_BG),
]
for i, (title, body, c, bg) in enumerate(steps):
    x = 40 + i * 272
    card(x, 700, 232, 130, title, body, c, bg)
    if i < len(steps) - 1:
        arrow(x + 236, 765, x + 268, 765)

text(40, 866, "Storage", size=14)
box(40, 892, 512, 110, stroke=MUTED, bg=GREY_BG)
text(56, 906, "study.db  (SQLite, planned)", size=13, color=MUTED)
text(56, 930, "saved_term(term, reading, gloss, book, chapter, sentence, status)\n"
              "saved_kanji(char, status)      term_kanji(term_id, char)\n"
              "export_log(book_id, term_ids)  — stops re-exporting", size=11)

text(600, 866, "Why kanji AND words", size=14)
box(600, 892, 500, 110, stroke=MUTED, bg=GREY_BG)
text(616, 910, "Saving a word answers \"what does this mean\".\n"
               "Indexing its kanji answers \"which characters do I\nnot know\" — and lets the same character light up\n"
               "inside a different word in a different book.", size=11)

text(1150, 866, "Highlighting must not", size=14)
box(1150, 892, 490, 110, stroke=MUTED, bg=GREY_BG)
text(1166, 910, "· match inside furigana (skip rt / rp nodes)\n"
                "· inject into HTML attributes (use a TreeWalker,\n   not indexOf on markup — the inherited bug)\n"
                "· re-scan on every render (memoise per block+ETag)", size=11)

# ── Panel 4: server ─────────────────────────────────────────────────────────
panel(0, 1100, 1680, 400, "4. Server", "One binary. The React PWA is embedded at compile time.")

box(40, 1180, 300, 250, stroke=BLUE, bg=BLUE_BG, strokeWidth=2)
text(56, 1194, "Caddy", size=15, color=BLUE)
text(56, 1218, "TLS :443, Let's Encrypt\nreverse proxy → :4567", size=12)
text(56, 1270, "EC2 t2.micro\n1 GB RAM — add 2 GB swap\n20–30 GB volume (dictionaries)", size=11, color=MUTED)

box(400, 1180, 1240, 250, stroke=INK, strokeWidth=2)
text(416, 1194, "lanobe  —  axum, ~21 MB binary, ~17 MB idle RSS", size=15)

crates = [
    ("app-server", "/api/app", "auth + settings", "app.db", GREEN),
    ("novel-server", "/api/novel", "library, progress", "novel.db (sled)", GREEN),
    ("yomitan-server", "/api/yomitan", "lookup, deinflect", "yomitan.db", GREEN),
    ("audio-server", "/api/audio", "word audio", "—", GREEN),
    ("study-server", "/api/study", "saved vocab", "study.db", AMBER),
]
for i, (name, route, what, db, c) in enumerate(crates):
    x = 420 + i * 242
    box(x, 1230, 226, 120, stroke=c, bg=AMBER_BG if c == AMBER else "transparent")
    text(x + 12, 1242, name, size=13, color=c)
    text(x + 12, 1264, route, size=11, color=MUTED)
    text(x + 12, 1288, what, size=11)
    text(x + 12, 1312, db, size=11, color=MUTED)
    if c == AMBER:
        text(x + 12, 1330, "P3 — not built", size=10, color=AMBER)

text(420, 1366, "/*  →  embedded React PWA (rust-embed).  Unknown /api/* returns JSON 404, never the SPA.", size=12, color=MUTED)
arrow(344, 1300, 396, 1300, color=BLUE)

doc = {
    "type": "excalidraw",
    "version": 2,
    "source": "https://github.com/RifkyHernanda/Lanobe",
    "elements": elements,
    "appState": {"gridSize": None, "viewBackgroundColor": "#ffffff"},
    "files": {},
}

out = Path(__file__).resolve().parent.parent / ".claude" / "ui-layout.excalidraw"
out.parent.mkdir(parents=True, exist_ok=True)
out.write_text(json.dumps(doc, indent=2, ensure_ascii=False), encoding="utf-8")
print(f"wrote {out} with {len(elements)} elements")
