"""Download the Mahjong Soul character icons listed on mahjongsoul.info.

    python scripts/fetch_characters.py [out_dir]

Reads https://mahjongsoul.info/characters/ (robots.txt allows it), takes each
character's 250x250 icon and name, and writes into `out_dir` (default
`assets/characters/`):

    <slug>.png     the icon (slug = the site's page name, e.g. `ichihime`)
    index.json     [{slug, name, section, file, page, source}] in the site's order

Polite on purpose: one request at a time with a pause, and files already
downloaded are skipped, so it can be run again to pick up new characters.

The pictures are the game's artwork (Yostar / Catfood), re-hosted by a fan
site. They are for local use; `assets/characters/` is git-ignored — do not
commit or ship them without the rights holder's permission.
"""

import html
import json
import re
import sys
import time
import urllib.request
from pathlib import Path

PAGE = "https://mahjongsoul.info/characters/"
UA = "Mozilla/5.0 (Mokora character icon fetch; personal use)"
PAUSE_S = 0.4


def get(url: str) -> bytes:
    req = urllib.request.Request(url, headers={"User-Agent": UA})
    with urllib.request.urlopen(req, timeout=30) as r:
        return r.read()


def parse(page: str):
    """Yield (slug, name, section, page_url, image_url) in page order."""
    # Section headings and character links, in document order.
    token = re.compile(
        r'<div class="cap_box_ttl">(?P<h>.*?)</div>'
        r'|<a[^>]+href="(?P<url>https://mahjongsoul\.info/database_(?P<slug>[^"/]+)/?)"[^>]*>(?P<body>.*?)</a>',
        re.S,
    )
    section = ""
    seen = {}
    order = []
    for m in token.finditer(page):
        if m.group("h") is not None:
            text = re.sub(r"<[^>]+>", "", m.group("h")).strip()
            if text:
                section = html.unescape(text)
            continue
        if not section:
            continue  # the banner above the first group is not a character
        slug, body = m.group("slug"), m.group("body")
        img = re.search(r'data-src="([^"]+\.(?:png|jpe?g|webp))"', body)
        text = html.unescape(re.sub(r"<[^>]+>", "", body)).strip()
        entry = seen.get(slug)
        if entry is None:
            entry = {"slug": slug, "name": "", "section": section, "page": m.group("url"), "source": ""}
            seen[slug] = entry
            order.append(entry)
        if img and not entry["source"]:
            entry["source"] = img.group(1)
        if text and not entry["name"]:
            entry["name"] = text
    return [e for e in order if e["source"]]


def main():
    out = Path(sys.argv[1]) if len(sys.argv) > 1 else Path(__file__).resolve().parent.parent / "assets" / "characters"
    out.mkdir(parents=True, exist_ok=True)
    entries = parse(get(PAGE).decode("utf-8", "replace"))
    print(f"{len(entries)} characters on the page")
    done = skipped = failed = 0
    for i, e in enumerate(entries, 1):
        ext = Path(e["source"].split("?")[0]).suffix or ".png"
        e["file"] = f"{e['slug']}{ext}"
        dest = out / e["file"]
        if dest.exists() and dest.stat().st_size > 0:
            skipped += 1
            continue
        try:
            dest.write_bytes(get(e["source"]))
            done += 1
        except Exception as exc:  # keep going; report at the end
            failed += 1
            e["file"] = ""
            print(f"  failed {e['slug']}: {exc}")
        time.sleep(PAUSE_S)
        if i % 20 == 0:
            print(f"  {i}/{len(entries)}")
    index = [e for e in entries if e["file"]]
    (out / "index.json").write_text(json.dumps(index, ensure_ascii=False, indent=2), encoding="utf-8")
    print(f"downloaded {done}, already there {skipped}, failed {failed} -> {out}")


if __name__ == "__main__":
    main()
