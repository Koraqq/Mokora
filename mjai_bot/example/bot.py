# Mokora v3 example bot: rule-based shanten optimizer.
#
# Algorithm summary:
#   AwaitingDiscard (own tsumo):
#     1. Tsumo agari if reachable.
#     2. Ankan / Kakan if shanten doesn't worsen.
#     3. Riichi when tenpai with >= 2 waits — pick the riichi maximising
#        wait count.
#     4. Discard: minimise shanten; tiebreak by effective ukeire
#        (remaining-in-walls count).
#   AwaitingResponse (others' dahai / kakan):
#     1. Ron if reachable.
#     2. Daiminkan / Pon / Chi when shanten strictly decreases AND a yaku
#        path exists. Daiminkan > Pon > Chi by precedence.
#     3. Otherwise pass.

from __future__ import annotations

import json
import os
import sys
from dataclasses import dataclass, field
from typing import Iterable

from mahjong.constants import EAST, NORTH, SOUTH, WEST
from mahjong.hand_calculating.hand import HandCalculator
from mahjong.hand_calculating.hand_config import HandConfig
from mahjong.meld import Meld
from mahjong.shanten import Shanten

# ---------- Mokora frontend notifications ----------
#
# Mokora reads the bot's stdout strictly as one mjai reaction per line, so a
# bot must NOT print anything else there. stderr, however, is a free-form
# side-channel: any line prefixed with NOTIFY_PREFIX followed by a JSON
# object is parsed by Mokora and surfaced as a toast in the app's bottom-right
# corner. Every other stderr line is treated as a normal diagnostic log line.
#
# Severity levels: "info", "success", "warn", "error".

NOTIFY_PREFIX = "@@MOKORA_NOTIFY@@ "


def notify(
    level: str,
    title: str,
    body: str | None = None,
    *,
    sticky: bool = False,
    id: str | None = None,
) -> None:
    """Send a toast notification to the Mokora frontend.

    Args:
        level: one of "info", "success", "warn", "error".
        title: short headline shown in bold.
        body: optional longer description.
        sticky: if True the toast stays until dismissed (use for long-running
            states); otherwise it auto-dismisses.
        id: stable key — a later notification with the same id replaces the
            earlier toast instead of stacking a new one (use for progress).
    """
    payload: dict[str, object] = {"level": level, "title": title}
    if body is not None:
        payload["body"] = body
    if sticky:
        payload["sticky"] = True
    if id is not None:
        payload["id"] = id
    sys.stderr.write(NOTIFY_PREFIX + json.dumps(payload, separators=(",", ":")) + "\n")
    sys.stderr.flush()


# ---------- mjai tile string ↔ 34-array index ----------

# 0-8 m, 9-17 p, 18-26 s, 27 E, 28 S, 29 W, 30 N, 31 P, 32 F, 33 C
_HONOR = {"E": 27, "S": 28, "W": 29, "N": 30, "P": 31, "F": 32, "C": 33}
_HONOR_REV = {v: k for k, v in _HONOR.items()}
_WIND_MJAI = {EAST: "E", SOUTH: "S", WEST: "W", NORTH: "N"}


def _normalize(tile: str) -> str:
    """Drop akadora 'r' suffix for shanten/identity purposes."""
    return tile[:-1] if tile.endswith("r") else tile


def _to34(tile: str) -> int:
    t = _normalize(tile)
    if t in _HONOR:
        return _HONOR[t]
    n = int(t[0])
    if n == 0:
        n = 5
    suit = t[1]
    return {"m": 0, "p": 9, "s": 18}[suit] + (n - 1)


def _from34(idx: int) -> str:
    if idx >= 27:
        return _HONOR_REV[idx]
    suit = "mps"[idx // 9]
    return f"{(idx % 9) + 1}{suit}"


def _is_yaochuu(idx: int) -> bool:
    if idx >= 27:
        return True
    n = idx % 9
    return n == 0 or n == 8


# ---------- 34 → 136 conversion ----------

def _hand34_to_136(counts34: list[int], aka_in_hand: dict[str, int]) -> list[int]:
    """Concealed-hand 34-array → 136-array. Akadora occupy slot 16,
    52, 88 (the first index of 5m, 5p, 5s blocks)."""
    used: dict[int, int] = {}
    out: list[int] = []
    aka_remaining = dict(aka_in_hand)
    for idx, n in enumerate(counts34):
        for _ in range(n):
            base = idx * 4
            slot = used.get(idx, 0)
            tile136 = base + slot
            # Promote slot 0 to akadora form if this is a red 5 owed.
            if idx in (4, 13, 22):  # 5m, 5p, 5s
                suit = "mps"[idx // 9]
                if aka_remaining.get(suit, 0) > 0 and slot == 0:
                    aka_remaining[suit] -= 1
            out.append(tile136)
            used[idx] = slot + 1
    return out


def _tile136_for_index(idx: int) -> int:
    return idx * 4


# ---------- meld → mahjong.Meld ----------

def _meld_to_mahjong(meld: dict) -> Meld:
    """Translate our internal meld dict into a mahjong.Meld. Tile
    indices are coarse 4*idx+0; mahjong only inspects tiles_34 for most
    yaku checks so the exact 136 slot doesn't matter here."""
    tiles136 = [_tile136_for_index(_to34(t)) for t in meld["tiles"]]
    if meld["type"] == "chi":
        return Meld(meld_type=Meld.CHI, tiles=tiles136, opened=True)
    if meld["type"] == "pon":
        return Meld(meld_type=Meld.PON, tiles=tiles136, opened=True)
    if meld["type"] == "daiminkan":
        return Meld(meld_type=Meld.KAN, tiles=tiles136, opened=True)
    if meld["type"] == "kakan":
        return Meld(meld_type=Meld.SHOUMINKAN, tiles=tiles136, opened=True)
    if meld["type"] == "ankan":
        return Meld(meld_type=Meld.KAN, tiles=tiles136, opened=False)
    raise ValueError(f"unknown meld type: {meld['type']}")


# ---------- state ----------

@dataclass
class State:
    actor_id: int = 0
    oya: int = 0
    bakaze: str = "E"
    kyoku: int = 1
    honba: int = 0
    kyotaku: int = 0
    # Concealed hand only (melds tracked separately).
    hand34: list[int] = field(default_factory=lambda: [0] * 34)
    aka_in_hand: dict[str, int] = field(default_factory=lambda: {"m": 0, "p": 0, "s": 0})
    melds: list[dict] = field(default_factory=list)
    rivers: list[list[str]] = field(default_factory=lambda: [[] for _ in range(4)])
    river_called: list[list[bool]] = field(default_factory=lambda: [[] for _ in range(4)])
    last_self_tsumo: str = ""
    reach_declared: bool = False
    reach_accepted: bool = False
    left_tiles: int = 70
    dora_indicators: list[str] = field(default_factory=list)

    # ----- mutators -----

    def consume(self, e: dict) -> None:
        t = e.get("type")
        if t == "start_game":
            if "id" in e:
                self.actor_id = int(e["id"])
            # Demonstrates the frontend notification channel — a toast pops in
            # the app's bottom-right corner. Remove or repurpose at will.
            notify(
                "info",
                "Example bot ready",
                f"Seated at player {self.actor_id}.",
            )
        elif t == "start_kyoku":
            self.oya = e["oya"]
            self.bakaze = e["bakaze"]
            self.kyoku = e["kyoku"]
            self.honba = e["honba"]
            self.kyotaku = e["kyotaku"]
            self.hand34 = [0] * 34
            self.aka_in_hand = {"m": 0, "p": 0, "s": 0}
            for tile in e["tehais"][self.actor_id]:
                self._add(tile)
            self.melds = []
            self.rivers = [[] for _ in range(4)]
            self.river_called = [[] for _ in range(4)]
            self.last_self_tsumo = ""
            self.reach_declared = False
            self.reach_accepted = False
            self.left_tiles = 70
            self.dora_indicators = [e["dora_marker"]]
        elif t == "dora":
            self.dora_indicators.append(e["dora_marker"])
        elif t == "tsumo":
            if e["actor"] == self.actor_id:
                self._add(e["pai"])
                self.last_self_tsumo = e["pai"]
            self.left_tiles = max(0, self.left_tiles - 1)
        elif t == "dahai":
            actor = e["actor"]
            self.rivers[actor].append(e["pai"])
            self.river_called[actor].append(False)
            if actor == self.actor_id:
                self._remove(e["pai"])
                self.last_self_tsumo = ""
        elif t == "reach_accepted":
            if e["actor"] == self.actor_id:
                self.reach_accepted = True
        elif t in ("chi", "pon", "daiminkan"):
            target = e["target"]
            if self.river_called[target]:
                self.river_called[target][-1] = True
            if e["actor"] == self.actor_id:
                for c in e["consumed"]:
                    self._remove(c)
                self.melds.append({
                    "type": t,
                    "tiles": list(e["consumed"]) + [e["pai"]],
                    "from_seat": target,
                    "called_tile": e["pai"],
                })
        elif t == "kakan":
            if e["actor"] == self.actor_id:
                self._remove(e["pai"])
                for m in self.melds:
                    if m["type"] == "pon" and _normalize(m["tiles"][0]) == _normalize(e["pai"]):
                        m["type"] = "kakan"
                        m["tiles"].append(e["pai"])
                        break
        elif t == "ankan":
            if e["actor"] == self.actor_id:
                for c in e["consumed"]:
                    self._remove(c)
                self.melds.append({
                    "type": "ankan",
                    "tiles": list(e["consumed"]),
                    "from_seat": e["actor"],
                })

    def _add(self, tile: str) -> None:
        if tile == "?":
            # Unknown tile (other seats' hidden hand). Should never reach
            # us for our own seat; skip defensively.
            return
        self.hand34[_to34(tile)] += 1
        if tile.endswith("r"):
            self.aka_in_hand[_normalize(tile)[1]] += 1

    def _remove(self, tile: str) -> None:
        if tile == "?":
            return
        idx = _to34(tile)
        if self.hand34[idx] > 0:
            self.hand34[idx] -= 1
        if tile.endswith("r") and self.aka_in_hand[_normalize(tile)[1]] > 0:
            self.aka_in_hand[_normalize(tile)[1]] -= 1

    # ----- queries -----

    def total_tiles(self) -> int:
        return sum(self.hand34) + sum(len(m["tiles"]) - (1 if m["type"] in ("ankan", "daiminkan", "kakan") else 0) for m in self.melds)

    def is_closed(self) -> bool:
        return all(m["type"] == "ankan" for m in self.melds)

    def seat_wind_mjai(self) -> str:
        return _WIND_MJAI[seat_wind_const(self.actor_id, self.oya)]

    def round_wind_mjai(self) -> str:
        return self.bakaze

    def kamicha(self) -> int:
        return (self.actor_id + 3) % 4

    def visible_count(self, idx: int) -> int:
        n = self.hand34[idx]
        for m in self.melds:
            for t in m["tiles"]:
                if _to34(t) == idx:
                    n += 1
        for s in range(4):
            for t in self.rivers[s]:
                if _to34(t) == idx:
                    n += 1
        for ind in self.dora_indicators:
            if _to34(ind) == idx:
                n += 1
        return n


def seat_wind_const(seat: int, oya: int) -> int:
    return [EAST, SOUTH, WEST, NORTH][(seat - oya) % 4]


# ---------- shanten / ukeire / yaku ----------

_SHANTEN = Shanten()
_CALC = HandCalculator()


def _shanten_with_melds(hand34: list[int], num_melds: int) -> int:
    # mahjong's calculate_shanten expects exactly 13 (or 14) tiles in the
    # 34-array. Open melds remove 3 tiles each from the concealed count.
    # We replicate the meld presence by padding with ghost honor triplets
    # so the algorithm sees a complete-shaped hand.
    #
    # Validity check is on `concealed + 3 * num_melds`, NOT raw concealed
    # count — with N open melds the concealed portion is only 13 - 3N
    # (or 14 - 3N right after a draw / call).
    total_with_melds = sum(hand34) + num_melds * 3
    if total_with_melds not in (13, 14):
        return 8  # malformed — return a deliberately bad score
    if num_melds == 0:
        return _SHANTEN.calculate_shanten(list(hand34))
    padded = list(hand34)
    ghost_slots = [27, 28, 29, 30, 31, 32, 33]
    placed = 0
    for h in ghost_slots:
        if placed >= num_melds:
            break
        if padded[h] == 0:
            padded[h] = 3
            placed += 1
    if placed < num_melds:
        return 8
    return _SHANTEN.calculate_shanten(padded)


def _ukeire(hand34: list[int], num_melds: int) -> list[int]:
    """Indices of tiles that strictly reduce shanten when added."""
    sh = _shanten_with_melds(hand34, num_melds)
    out = []
    for idx in range(34):
        if hand34[idx] >= 4:
            continue
        hand34[idx] += 1
        new_sh = _shanten_with_melds(hand34, num_melds)
        hand34[idx] -= 1
        if new_sh < sh:
            out.append(idx)
    return out


def _tenpai_waits(hand34: list[int], num_melds: int) -> list[int]:
    """If hand is tenpai (shanten == 0 with 13 tiles), return wait tiles."""
    sh = _shanten_with_melds(hand34, num_melds)
    if sh != 0:
        return []
    out = []
    for idx in range(34):
        if hand34[idx] >= 4:
            continue
        hand34[idx] += 1
        new_sh = _shanten_with_melds(hand34, num_melds)
        hand34[idx] -= 1
        if new_sh == -1:
            out.append(idx)
    return out


def _hand_value(state: State, hand34_full: list[int], aka_in_hand: dict[str, int],
                win_tile_idx: int, is_tsumo: bool):
    """Call mahjong's HandCalculator. hand34_full must include the
    winning tile and total to 14 tiles (concealed + meld). Returns the
    HandResponse — `.cost is not None` ⇔ valid (yaku present)."""
    tiles136 = _hand34_to_136(hand34_full, aka_in_hand)
    win_tile_136 = _tile136_for_index(win_tile_idx)
    melds = [_meld_to_mahjong(m) for m in state.melds if m["type"] != "ankan"]
    ankan_melds = [_meld_to_mahjong(m) for m in state.melds if m["type"] == "ankan"]
    # mahjong wants ankan in melds list with opened=False; pass them all
    melds.extend(ankan_melds)
    dora_136 = [_tile136_for_index(_to34(t)) for t in state.dora_indicators]
    config = HandConfig(
        is_tsumo=is_tsumo,
        is_riichi=state.reach_accepted and is_tsumo,
        player_wind=seat_wind_const(state.actor_id, state.oya),
        round_wind={"E": EAST, "S": SOUTH, "W": WEST}.get(state.bakaze, EAST),
    )
    return _CALC.estimate_hand_value(
        tiles=tiles136,
        win_tile=win_tile_136,
        melds=melds if melds else None,
        dora_indicators=dora_136 if dora_136 else None,
        config=config,
    )


def _has_obvious_yaku_path(
    state: State,
    hand34: list[int],
    extra_melds: list[dict] | None = None,
) -> bool:
    """Cheap heuristic: hand has a likely yaku path. Mirrors the C++
    `has_obvious_yaku_path`. `extra_melds` are hypothetical melds that
    haven't been added to `state.melds` yet (e.g. the pon we're
    considering)."""
    extras = extra_melds or []
    all_melds = list(state.melds) + extras
    sw = seat_wind_const(state.actor_id, state.oya) - EAST + 27  # 27..30
    rw = _to34(state.bakaze)

    def is_yakuhai(idx: int) -> bool:
        return idx >= 31 or idx == sw or idx == rw

    # Open meld of yakuhai
    for m in all_melds:
        if m["type"] != "chi" and is_yakuhai(_to34(m["tiles"][0])):
            return True
    # Concealed triplet of yakuhai
    for idx in range(34):
        if hand34[idx] >= 3 and is_yakuhai(idx):
            return True
    # Tanyao path
    all_simple = True
    for idx in range(34):
        if hand34[idx] > 0 and _is_yaochuu(idx):
            all_simple = False
            break
    if all_simple:
        for m in all_melds:
            for t in m["tiles"]:
                if _is_yaochuu(_to34(t)):
                    all_simple = False
                    break
            if not all_simple:
                break
    if all_simple:
        return True
    # Single-suit progress (honitsu/chinitsu)
    has_suit = [False, False, False]
    for idx in range(27):
        if hand34[idx] > 0:
            has_suit[idx // 9] = True
    for m in all_melds:
        for t in m["tiles"]:
            i = _to34(t)
            if i < 27:
                has_suit[i // 9] = True
    if sum(has_suit) <= 1:
        return True
    return False


def _has_viable_yaku(
    state: State,
    hand13_34: list[int],
    aka: dict[str, int],
    extra_melds: list[dict] | None = None,
) -> bool:
    """At tenpai, try each wait via HandCalculator; otherwise heuristic.
    `extra_melds` lets callers evaluate a hypothetical post-call hand
    without mutating `state.melds`."""
    extras = extra_melds or []
    num_melds = len(state.melds) + len(extras)
    sh = _shanten_with_melds(hand13_34, num_melds)
    if sh == 0:
        # Tenpai: try each wait. We don't pass extras to HandCalculator
        # because constructing the right 136-array with hypothetical
        # melds is fiddly; the heuristic path below is good enough for
        # the rare "tenpai with a hypothetical call" case (it would
        # only matter if you could call into agari, which mjai handles
        # via daiminkan→rinshan separately).
        for w in _tenpai_waits(hand13_34, num_melds):
            if extras:
                if _has_obvious_yaku_path(state, hand13_34, extras):
                    return True
                continue
            full = list(hand13_34)
            full[w] += 1
            res = _hand_value(state, full, aka, w, is_tsumo=False)
            if res.cost is not None:
                return True
        return False
    return _has_obvious_yaku_path(state, hand13_34, extras)


def _effective_ukeire(state: State, hand13_34: list[int]) -> int:
    total = 0
    for idx in _ukeire(hand13_34, len(state.melds)):
        rem = 4 - state.visible_count(idx)
        if rem > 0:
            total += rem
    return total


# ---------- mjai actions ----------

def _act_none(actor: int) -> dict:
    return {"type": "none"}


def _pick_consumed_for_pon(state: State, called_idx: int) -> list[str] | None:
    """Pick exactly two tiles from concealed hand to pon `called_idx`.
    Returns mjai tile strings, or None if pon not possible."""
    if state.hand34[called_idx] < 2:
        return None
    base = _from34(called_idx)
    consumed: list[str] = []
    suit = base[1] if base[0].isdigit() else ""
    aka_avail = state.aka_in_hand.get(suit, 0) if base[0] == "5" and suit in "mps" else 0
    if aka_avail >= 1 and state.hand34[called_idx] >= 2:
        consumed.append(f"5{suit}r")
        consumed.append(f"5{suit}")
    else:
        consumed.append(base)
        consumed.append(base)
    return consumed


def _pick_consumed_for_daiminkan(state: State, called_idx: int) -> list[str] | None:
    if state.hand34[called_idx] < 3:
        return None
    base = _from34(called_idx)
    if base[0] == "5" and base[1] in "mps":
        suit = base[1]
        if state.aka_in_hand.get(suit, 0) >= 1:
            return [f"5{suit}r", base, base]
    return [base, base, base]


def _chi_options(state: State, called_idx: int) -> list[list[str]]:
    """Possible chi consumed-pair lists for called_idx (mjai strings)."""
    if called_idx >= 27:
        return []
    suit = "mps"[called_idx // 9]
    n = called_idx % 9 + 1
    pairs = []
    for a, b in [(n - 2, n - 1), (n - 1, n + 1), (n + 1, n + 2)]:
        if a < 1 or b > 9 or a > 9 or b < 1:
            continue
        idx_a = (called_idx // 9) * 9 + (a - 1)
        idx_b = (called_idx // 9) * 9 + (b - 1)
        if state.hand34[idx_a] < 1 or state.hand34[idx_b] < 1:
            continue
        # Use red 5 form when the pair includes a 5 and we hold an aka
        def name(idx: int, num: int) -> str:
            if num == 5 and state.aka_in_hand.get(suit, 0) > 0:
                return f"5{suit}r"
            return f"{num}{suit}"
        pairs.append([name(idx_a, a), name(idx_b, b)])
    return pairs


# ---------- decision: own tsumo ----------

def decide_after_own_tsumo(state: State, last_event: dict) -> dict:
    actor = state.actor_id
    drawn = last_event["pai"]
    drawn_idx = _to34(drawn)

    # Rule 0: Tsumo agari
    hand14 = list(state.hand34)
    res = _hand_value(state, hand14, state.aka_in_hand, drawn_idx, is_tsumo=True)
    if res.cost is not None:
        return {"type": "hora", "actor": actor, "target": actor, "pai": drawn}

    # Rule: Ankan if shanten doesn't worsen
    if not state.reach_accepted:
        sh14 = _shanten_with_melds(hand14, len(state.melds))
        for idx in range(34):
            if hand14[idx] >= 4:
                # Build the post-ankan concealed hand and re-check shanten
                test = list(hand14)
                test[idx] = 0
                aka_keep = dict(state.aka_in_hand)
                if idx in (4, 13, 22):
                    aka_keep["mps"[idx // 9]] = 0
                # Treat ankan as one more meld for shanten purposes
                if _shanten_with_melds(test, len(state.melds) + 1) <= sh14:
                    base = _from34(idx)
                    consumed = [base, base, base, base]
                    if base[0] == "5" and base[1] in "mps":
                        suit = base[1]
                        if state.aka_in_hand.get(suit, 0) >= 1:
                            consumed = [f"5{suit}r", base, base, base]
                    return {"type": "ankan", "actor": actor, "consumed": consumed}

        # Rule: Kakan if shanten doesn't worsen
        for m in state.melds:
            if m["type"] != "pon":
                continue
            tile = m["tiles"][0]
            tidx = _to34(tile)
            if hand14[tidx] >= 1:
                test = list(hand14)
                test[tidx] -= 1
                if _shanten_with_melds(test, len(state.melds)) <= sh14:
                    pai = _from34(tidx)
                    if pai[0] == "5" and pai[1] in "mps" and state.aka_in_hand.get(pai[1], 0) > 0:
                        pai = f"5{pai[1]}r"
                    return {
                        "type": "kakan",
                        "actor": actor,
                        "pai": pai,
                        "consumed": list(m["tiles"]),
                    }

    # Already in riichi → tsumogiri
    if state.reach_accepted:
        return {
            "type": "dahai",
            "actor": actor,
            "pai": drawn,
            "tsumogiri": True,
        }

    # Rule 3: Riichi maximising waits
    best_riichi: tuple[int, str] | None = None  # (wait_count, mjai_tile_to_discard)
    if state.is_closed() and not state.reach_declared and state.left_tiles >= 4:
        for idx in range(34):
            if hand14[idx] == 0:
                continue
            test = list(hand14)
            test[idx] -= 1
            if _shanten_with_melds(test, len(state.melds)) != 0:
                continue
            waits = _tenpai_waits(test, len(state.melds))
            nw = len(waits)
            if best_riichi is None or nw > best_riichi[0]:
                # Choose mjai tile name (prefer non-aka if both exist)
                pai = _from34(idx)
                if idx in (4, 13, 22):
                    suit = "mps"[idx // 9]
                    # If we ONLY have aka, discard aka
                    if state.aka_in_hand.get(suit, 0) > 0 and hand14[idx] - state.aka_in_hand.get(suit, 0) == 0:
                        pai = f"5{suit}r"
                best_riichi = (nw, pai)
        if best_riichi and best_riichi[0] >= 2:
            return {"type": "reach", "actor": actor}

    # Rule 1: Discard minimising shanten, tiebreak by effective ukeire
    best: tuple[int, int, str] | None = None  # (shanten, -ukeire, pai_str)
    for idx in range(34):
        if hand14[idx] == 0:
            continue
        test = list(hand14)
        test[idx] -= 1
        sh = _shanten_with_melds(test, len(state.melds))
        uke = _effective_ukeire(state, test)
        pai = _from34(idx)
        if idx in (4, 13, 22):
            suit = "mps"[idx // 9]
            if state.aka_in_hand.get(suit, 0) > 0 and hand14[idx] - state.aka_in_hand.get(suit, 0) == 0:
                pai = f"5{suit}r"
        cand = (sh, -uke, pai, idx)
        if best is None or cand < best:
            best = cand
    if best is not None:
        sh, _u, pai, idx = best
        # Detect tsumogiri: discard equals drawn tile
        tsumogiri = pai == drawn
        return {
            "type": "dahai",
            "actor": actor,
            "pai": pai,
            "tsumogiri": tsumogiri,
        }

    # Fallback
    return _act_none(actor)


# ---------- decision: response to others ----------

def decide_after_others_dahai(state: State, last_event: dict) -> dict:
    actor = state.actor_id
    discarder = last_event["actor"]
    discarded = last_event["pai"]
    didx = _to34(discarded)

    # Furiten skip: if we already discarded the wait tile.
    own_river_idxs = {_to34(t) for t in state.rivers[actor]}

    # Rule 0: Ron
    hand14 = list(state.hand34)
    hand14[didx] += 1
    if _shanten_with_melds(hand14, len(state.melds)) == -1 and didx not in own_river_idxs:
        # Build a temporary aka view for the call (no aka added by someone else's discard,
        # unless the discarded tile itself is an aka — track via mjai tile string)
        aka_in = dict(state.aka_in_hand)
        if discarded.endswith("r"):
            aka_in[_normalize(discarded)[1]] = aka_in.get(_normalize(discarded)[1], 0) + 1
        res = _hand_value(state, hand14, aka_in, didx, is_tsumo=False)
        if res.cost is not None:
            return {
                "type": "hora",
                "actor": actor,
                "target": discarder,
                "pai": discarded,
            }

    if state.reach_accepted:
        # In riichi we cannot call. (Ankan would be allowed but we
        # already reach-accepted → no upgrade here.)
        return _act_none(actor)

    current_sh = _shanten_with_melds(state.hand34, len(state.melds))

    # ----- Daiminkan -----
    consumed = _pick_consumed_for_daiminkan(state, didx)
    if consumed is not None:
        post = list(state.hand34)
        post[didx] -= 3
        new_sh = _shanten_with_melds(post, len(state.melds) + 1)
        hypothetical = [{
            "type": "daiminkan",
            "tiles": [discarded] + consumed,
            "from_seat": discarder,
            "called_tile": discarded,
        }]
        if new_sh < current_sh and _has_viable_yaku(
            state, post, state.aka_in_hand, extra_melds=hypothetical
        ):
            return {
                "type": "daiminkan",
                "actor": actor,
                "target": discarder,
                "pai": discarded,
                "consumed": consumed,
            }

    # ----- Pon -----
    pon_consumed = _pick_consumed_for_pon(state, didx)
    if pon_consumed is not None:
        post = list(state.hand34)
        post[didx] -= 2
        best_sh = 99
        best_post = None
        for j in range(34):
            if post[j] == 0:
                continue
            t = list(post)
            t[j] -= 1
            sh = _shanten_with_melds(t, len(state.melds) + 1)
            if sh < best_sh:
                best_sh = sh
                best_post = t
        hypothetical = [{
            "type": "pon",
            "tiles": list(pon_consumed) + [discarded],
            "from_seat": discarder,
            "called_tile": discarded,
        }]
        if best_post is not None and best_sh < current_sh and _has_viable_yaku(
            state, best_post, state.aka_in_hand, extra_melds=hypothetical
        ):
            return {
                "type": "pon",
                "actor": actor,
                "target": discarder,
                "pai": discarded,
                "consumed": pon_consumed,
            }

    # ----- Chi (only from kamicha) -----
    if discarder == state.kamicha():
        for chi_consumed in _chi_options(state, didx):
            post = list(state.hand34)
            for c in chi_consumed:
                post[_to34(c)] -= 1
            best_sh = 99
            best_post = None
            for j in range(34):
                if post[j] == 0:
                    continue
                t = list(post)
                t[j] -= 1
                sh = _shanten_with_melds(t, len(state.melds) + 1)
                if sh < best_sh:
                    best_sh = sh
                    best_post = t
            hypothetical = [{
                "type": "chi",
                "tiles": list(chi_consumed) + [discarded],
                "from_seat": discarder,
                "called_tile": discarded,
            }]
            if best_post is not None and best_sh < current_sh and _has_viable_yaku(
                state, best_post, state.aka_in_hand, extra_melds=hypothetical
            ):
                return {
                    "type": "chi",
                    "actor": actor,
                    "target": discarder,
                    "pai": discarded,
                    "consumed": chi_consumed,
                }

    return _act_none(actor)


def decide_after_others_kakan(state: State, last_event: dict) -> dict:
    """Chankan window. Same as ron but on kakan tile."""
    actor = state.actor_id
    declarer = last_event["actor"]
    tile = last_event["pai"]
    tidx = _to34(tile)
    own_river_idxs = {_to34(t) for t in state.rivers[actor]}
    hand14 = list(state.hand34)
    hand14[tidx] += 1
    if _shanten_with_melds(hand14, len(state.melds)) == -1 and tidx not in own_river_idxs:
        aka_in = dict(state.aka_in_hand)
        if tile.endswith("r"):
            aka_in[_normalize(tile)[1]] = aka_in.get(_normalize(tile)[1], 0) + 1
        res = _hand_value(state, hand14, aka_in, tidx, is_tsumo=False)
        if res.cost is not None:
            return {
                "type": "hora",
                "actor": actor,
                "target": declarer,
                "pai": tile,
            }
    return _act_none(actor)


# ---------- top-level ----------

def react(state: State, events_json: str) -> str:
    events = json.loads(events_json)
    if not events:
        return json.dumps({"type": "none"}, separators=(",", ":"))
    for e in events:
        state.consume(e)
    last = events[-1]
    t = last.get("type")
    actor = state.actor_id
    if t == "tsumo" and last.get("actor") == actor:
        action = decide_after_own_tsumo(state, last)
    elif t == "dahai" and last.get("actor") != actor:
        action = decide_after_others_dahai(state, last)
    elif t == "kakan" and last.get("actor") != actor:
        action = decide_after_others_kakan(state, last)
    else:
        action = {"type": "none"}
    return json.dumps(action, separators=(",", ":"))


def _initial_actor_id() -> int:
    if len(sys.argv) > 1:
        try:
            return int(sys.argv[1])
        except ValueError:
            pass
    env = os.environ.get("MOKORA_PLAYER_ID")
    if env is not None:
        try:
            return int(env)
        except ValueError:
            pass
    return 0


def main() -> None:
    state = State(actor_id=_initial_actor_id())
    for raw in sys.stdin:
        line = raw.strip()
        if not line:
            continue
        try:
            resp = react(state, line)
        except Exception as e:  # never crash the loop
            sys.stderr.write(f"bot error: {e}\n")
            sys.stderr.flush()
            resp = json.dumps({"type": "none"}, separators=(",", ":"))
        sys.stdout.write(resp + "\n")
        sys.stdout.flush()
        try:
            evs = json.loads(line)
        except Exception:
            continue
        if any(ev.get("type") == "end_game" for ev in evs):
            break


if __name__ == "__main__":
    main()
