"""Build the character icon table the app ships with.

    python scripts/fetch_characters.py            # icons from mahjongsoul.info
    python scripts/build_character_table.py       # this: names from the game + match

The game numbers its characters; its login traffic says which one is the
player's main character (`fetchCharacterInfo.main_character_id`) but not who
that is. The client's data names them in order (`Excels/Langs/
item_definition_character_name_<lang>`, one list per language) and every
character has an emote folder named by its id (`MyAssets/deco/emo/e<id>/`).
Sorted by id, those folders line up one-to-one with the names (the ids have a
few gaps, so the id is not simply the position). This script reads both from
the live Unity bundles (the way `extract_liqi.py` does; needs UnityPy and
requests), matches each name to an icon downloaded by `fetch_characters.py`, and
writes

    frontend/public/characters/<id>.png     128x128 icon of character <id>
    frontend/public/characters/index.json   {"<id>": {"jp", "zh", "en", "slug"}}

Characters with no icon are listed at the end; add an alias to `ALIASES` (game
name -> site name) when the two sites spell a name differently.
"""

import importlib.util
import json
import re
import sys
import unicodedata
import warnings
from pathlib import Path

from PIL import Image

ROOT = Path(__file__).resolve().parent.parent
ICONS = ROOT / "assets" / "characters"
OUT = ROOT / "frontend" / "public" / "characters"
LANGS = {"jp": "jp", "zh": "chs_t", "en": "en"}
NAME_ASSET = "LuaByte/Lua/Excels/Langs/item_definition_character_name_{lang}.lua.bytes"

# The game's spelling -> the icon site's, where they differ beyond spaces and
# width (checked by hand against the page).
ALIASES: dict[str, str] = {"ルルーシュ・ランペルージ": "ルルーシュ"}


def _load_extractor():
    spec = importlib.util.spec_from_file_location("xl", ROOT / "scripts" / "extract_liqi.py")
    xl = importlib.util.module_from_spec(spec)
    sys.modules["xl"] = xl
    spec.loader.exec_module(xl)
    return xl


def fetch_names() -> tuple[dict[str, list[str]], list[int]]:
    """({lang: [name of the first character, ...]}, [character ids, ascending])."""
    import UnityPy

    xl = _load_extractor()
    UnityPy.config.FALLBACK_UNITY_VERSION = xl.UNITY_FALLBACK_VERSION
    warnings.filterwarnings("ignore")
    s = xl._session()
    html = xl._get_bytes(s, xl.CLIENT_URL).decode("utf-8", "replace")
    issuer = re.search(r'Build/([^"\']+?\.loader\.js)', html).group(1).split("-WebGL-release-", 1)[0]
    cbs = json.loads(xl._get_bytes(s, xl._join(xl._origin(xl.CLIENT_URL), f"assetbundles/clientBundleSettings/{issuer}-release.json")))
    wh = cbs["warehouses"][0]
    whs = json.loads(xl._get_bytes(s, xl._join(xl._choose_url(wh["urls"]), wh["warehouseSettingPath"])))
    base = xl._join(xl._choose_url(whs["urls"]), whs["bundlePath"]).rstrip("/") + "/DXT/"
    bundles, assets = xl._read_bundle_info(UnityPy, xl._get_bytes(s, base + "bundle_info_so.majset"))
    wanted = {NAME_ASSET.format(lang=v): k for k, v in LANGS.items()}
    owner: dict[str, list[str]] = {}
    ids: set[int] = set()
    for a in assets:
        p = a.get("assetPath")
        m = re.match(r"MyAssets/deco/emo/e(\d+)/", p) if isinstance(p, str) else None
        if m:
            ids.add(int(m.group(1)))
        if p in wanted:
            owner.setdefault(bundles[a["ownerBundleIndex"]]["name"], []).append(p)
    out: dict[str, list[str]] = {}
    for name, paths in owner.items():
        got = xl._extract_text_assets(UnityPy, xl._get_bytes(s, base + name, 120))
        for p in paths:
            text = xl._decode_lua(xl._lookup_asset(got, p)).decode("utf-8", "surrogateescape")
            # `local a={"一姫","二階堂美樹",...}return a`
            body = text[text.index("{") + 1 : text.rindex("}")]
            out[wanted[p]] = json.loads("[" + body + "]")
    missing = set(LANGS) - set(out)
    if missing:
        raise SystemExit(f"name lists not found: {missing}")
    ordered = sorted(ids)
    if len(ordered) != len(out["jp"]):
        raise SystemExit(f"{len(ordered)} character ids but {len(out['jp'])} names: cannot pair them")
    return out, ordered


def norm(name: str) -> str:
    name = unicodedata.normalize("NFKC", name)
    name = re.sub(r"[（(].*?[）)]|【.*?】", "", name)
    return re.sub(r"\s+", "", name).lower()


def main():
    names, ids = fetch_names()
    jp = names["jp"]
    print(f"{len(jp)} characters in the client")
    site = json.loads((ICONS / "index.json").read_text(encoding="utf-8"))
    by_name = {norm(e["name"]): e for e in site}

    OUT.mkdir(parents=True, exist_ok=True)
    table, unmatched, used = {}, [], set()
    for old in OUT.glob("*.png"):
        old.unlink()
    for n, (cid, jname) in enumerate(zip(ids, jp), 1):
        entry = by_name.get(norm(ALIASES.get(jname, jname)))
        if entry is None:
            unmatched.append((cid, jname))
            continue
        used.add(entry["slug"])
        img = Image.open(ICONS / entry["file"]).convert("RGBA")
        side = min(img.size)
        img = img.crop(((img.width - side) // 2, (img.height - side) // 2, (img.width + side) // 2, (img.height + side) // 2))
        img.resize((128, 128), Image.LANCZOS).save(OUT / f"{cid}.png", optimize=True)
        table[str(cid)] = {
            "jp": jname,
            "zh": names["zh"][n - 1] if n <= len(names["zh"]) else "",
            "en": names["en"][n - 1] if n <= len(names["en"]) else "",
            "slug": entry["slug"],
        }
    (OUT / "index.json").write_text(json.dumps(table, ensure_ascii=False, indent=1), encoding="utf-8")
    print(f"matched {len(table)} -> {OUT}")
    if unmatched:
        print("no icon for:", ", ".join(f"{cid}:{name}" for cid, name in unmatched))
    leftovers = [e["name"] for e in site if e["slug"] not in used]
    if leftovers:
        print("icons not used:", ", ".join(leftovers))


if __name__ == "__main__":
    main()
