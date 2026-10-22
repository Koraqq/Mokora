-- Mokora autoplay delay model.
--
-- Mokora generated this file next to your config.toml. Edit it freely —
-- it hot-reloads every time you save. Delete it to get a fresh copy of
-- the default on the next start. If the script ever errors, Mokora keeps
-- playing with its built-in model and logs the reason once.
--
-- decide_delay(ctx) returns { delay_ms = <number>, allow_bank = <bool> }.
--
-- delay_ms is the target TOTAL thinking time for the decision window —
-- the time the SERVER observes between offering the decision and
-- receiving the action. It is NOT a sleep length: Mokora subtracts
-- whatever networking and bot inference already consumed.
--
-- Mokora always enforces on top of whatever this script returns:
--   * a minimum delay (config `min_delay_ms`) — clicks issued before
--     Mahjong Soul finishes rendering buttons/tiles are silently lost
--   * the dealing-animation wait on the dealer's opening discard
--   * the server time budget — autoplay can never run into an
--     auto-discard; the extra time bank is only spent when this script
--     returns allow_bank = true
--
-- Context passed to decide_delay:
--   ctx.action        "dahai" | "reach" | "chi" | "pon" | "daiminkan"
--                     | "ankan" | "kakan" | "hora" | "ryukyoku" | "kita"
--                     | "none"  (declining a call window)
--   ctx.tsumogiri     the discard is the just-drawn tile
--   ctx.post_call     discard following our own chi/pon
--   ctx.first_action  first action of the kyoku
--   ctx.dealer_opening dealer's 14-tile opening discard
--   ctx.can_riichi    riichi is declarable this turn
--   ctx.is_kan        the action is a kan declaration
--   ctx.in_riichi     we are in accepted riichi
--   ctx.junme         our discard number this kyoku (1 = first)
--   ctx.legal_count   number of legal actions
--   ctx.top_prob      bot's top candidate probability (0..1), or nil
--   ctx.second_prob   second candidate probability, or nil
--   ctx.margin        top_prob - second_prob, or nil
--   ctx.budget        { fixed_ms, add_ms, elapsed_ms } or nil
--   ctx.rng()         uniform random in [0, 1)
--   ctx.lognormal(mu, sigma)  log-normal sample in SECONDS
--
-- The base numbers below are calibrated against ranked Throne-room game
-- records (~4100 decisions, think times measured on the server clock).

-- Per-decision log-normal parameters { mu, sigma } in ln-seconds.
-- Median think time = e^mu seconds.
local BASE = {
  dahai_tedashi   = { 0.90, 0.60 }, -- discard from hand      (median ~2.4s)
  dahai_tsumogiri = { 0.62, 0.55 }, -- discard the drawn tile (median ~1.9s)
  post_call_dahai = { 0.44, 0.35 }, -- discard after own call (median ~1.5s)
  reach           = { 1.00, 0.45 }, -- riichi declaration     (median ~2.7s)
  claim           = { 0.27, 0.55 }, -- call window, incl pass (median ~1.3s)
  hora            = { 0.15, 0.50 }, -- ron / tsumo            (median ~1.2s)
}

local function base_key(ctx)
  if ctx.action == "dahai" then
    if ctx.post_call then return "post_call_dahai" end
    if ctx.tsumogiri then return "dahai_tsumogiri" end
    return "dahai_tedashi"
  end
  if ctx.action == "reach" then return "reach" end
  if ctx.action == "hora" then return "hora" end
  -- Own-turn kan declarations get tedashi-like thought; every claim
  -- window response (chi/pon/daiminkan/skip/kyuushu) shares the fast
  -- reaction-window distribution.
  if ctx.action == "ankan" or ctx.action == "kakan" then
    return "dahai_tedashi"
  end
  return "claim"
end

function decide_delay(ctx)
  -- In riichi there is nothing left to decide — the only windows that
  -- reach this script are call windows we are about to decline. Humans
  -- answer these almost instantly (the host minimum still applies).
  if ctx.in_riichi then
    return { delay_ms = 700 + 500 * ctx.rng() }
  end

  local p = BASE[base_key(ctx)]
  local think = ctx.lognormal(p[1], p[2])
  local allow_bank = false

  -- Humans slow down as the hand develops (more to read on the table):
  -- measured ~+20% median between early and mid game.
  if ctx.junme ~= nil and ctx.junme > 0 then
    think = think * (1.0 + 0.015 * math.min(ctx.junme, 15))
  end

  -- First action of a kyoku: a fresh hand gets surveyed first.
  if ctx.first_action then
    think = think + 0.5 + 0.8 * ctx.rng()
  end

  -- A declarable riichi makes this discard a real decision, even when
  -- the bot ends up not declaring.
  if ctx.can_riichi and ctx.action == "dahai" then
    think = think + 0.4 + 0.8 * ctx.rng()
  end

  -- Difficulty, from the bot's own candidate probabilities.
  if ctx.margin ~= nil and ctx.margin < 0.02 then
    -- Genuinely close call: think visibly longer and let Mokora spend
    -- the time bank on it (it refills every kyoku).
    think = think + ctx.lognormal(0.6, 0.5)
    allow_bank = true
  elseif ctx.margin ~= nil and ctx.margin < 0.10 then
    think = think + ctx.lognormal(-0.2, 0.5)
  end
  if ctx.top_prob ~= nil and ctx.top_prob > 0.97 then
    -- Obvious move: humans snap these.
    think = think * 0.6
  end

  -- Occasional genuine tank — reading discards, counting payouts.
  -- Real players' p99 think times run 7-15s.
  if ctx.rng() < 0.03 then
    think = think + 2.0 + ctx.lognormal(0.8, 0.6)
    allow_bank = true
  end

  return { delay_ms = think * 1000, allow_bank = allow_bank }
end
