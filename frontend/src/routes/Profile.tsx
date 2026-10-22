// 個人資料: the logged-in player as the game's own traffic shows them — rank
// on each ladder, copper coins and jade, achievements, today's revive coins
// — and Mahjong Soul's own statistics of their ranked games, as its profile
// window shows them: placements, recent wins and deal-ins, the biggest hand
// (精選和牌), the recent trend, and every yaku won. Below, the game's own
// list of recent games (every game, played through Mokora or not), with the
// model that played each one where the history knows it. Read from
// `get_profile`, kept beside the account's config, so it shows offline too.

import { useRef } from 'react'
import { useTranslation } from 'react-i18next'
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card'
import { Mahgen } from '@/components/Mahgen'
import { useProfile } from '@/hooks/useProfile'
import {
  YAKU_IDS,
  YAKUMAN_IDS,
  isYakumanHan,
  levelParts,
  playersOfMode,
  recentGames,
  share,
  trendOf,
} from '@/lib/majsoulProfile'
import { majsoulRoomTier, roomLabelKey } from '@/lib/matchInfo'
import { nativeBotShortName } from '@/lib/nativeBots'
import { mjaiToMahgen } from '@/lib/tileIdx'
import { cn } from '@/lib/utils'
import { useHistoryStore } from '@/stores/historyStore'
import type { AccountProfile, HighestHand, LevelScore, MajsoulRankStats } from '@/types'

const RECENT_GAMES = 20
const TREND_GAMES = 10

export function Profile() {
  const { t } = useTranslation()
  const profile = useProfile()

  return (
    <div className="p-6 flex flex-col gap-6 w-full">
      <header>
        <h1 className="text-2xl font-semibold">
          {profile?.nickname ? t('profile.title_of', { name: profile.nickname }) : t('profile.title')}
        </h1>
      </header>
      {profile === undefined ? null : profile === null ? (
        <p className="text-sm text-muted-foreground">{t('profile.empty')}</p>
      ) : (
        <>
          <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
            <BasicCard profile={profile} />
            <TrendCard profile={profile} />
          </div>
          <StatsCard profile={profile} />
          <RecentCard profile={profile} />
        </>
      )}
    </div>
  )
}

function Row({ label, children }: { label: string; children: React.ReactNode }) {
  return (
    <div className="flex items-baseline justify-between py-1 border-b border-border/40 text-sm">
      <span className="text-muted-foreground">{label}</span>
      <span className="font-mono">{children}</span>
    </div>
  )
}

function useLevelLabel() {
  const { t } = useTranslation()
  return (level: LevelScore | null) => {
    if (!level) return '—'
    const { tier, star } = levelParts(level.id)
    return `${t(`profile.tier.${tier}`)} ${star} · ${level.score}pt`
  }
}

function BasicCard({ profile }: { profile: AccountProfile }) {
  const { t, i18n } = useTranslation()
  const levelLabel = useLevelLabel()
  const num = (n: number | null) => (n == null ? '—' : n.toLocaleString(i18n.language))
  const revive =
    profile.revive_claimed == null
      ? t('profile.revive_unknown')
      : profile.revive_claimed
        ? t('profile.revive_claimed')
        : t('profile.revive_available')
  const a = profile.achievements
  return (
    <Card>
      <CardHeader>
        <CardTitle className="text-sm uppercase tracking-wider">{t('profile.basic')}</CardTitle>
      </CardHeader>
      <CardContent className="grid gap-0.5">
        <Row label={t('profile.nickname')}>{profile.nickname ?? '—'}</Row>
        <Row label={t('profile.account_id')}>{profile.account_id ?? '—'}</Row>
        <Row label={t('profile.level3')}>{levelLabel(profile.level3)}</Row>
        <Row label={t('profile.level4')}>{levelLabel(profile.level4)}</Row>
        <Row label={t('profile.gold')}>{num(profile.gold)}</Row>
        <Row label={t('profile.diamond')}>{num(profile.diamond)}</Row>
        <Row label={t('profile.achievements')}>
          {a ? (
            <span className="flex gap-3">
              <span className="text-amber-400">{t('profile.ach_gold', { n: a.gold })}</span>
              <span className="text-zinc-300">{t('profile.ach_silver', { n: a.silver })}</span>
              <span className="text-orange-400">{t('profile.ach_bronze', { n: a.bronze })}</span>
            </span>
          ) : (
            '—'
          )}
        </Row>
        <Row label={t('profile.revive')}>{revive}</Row>
        <Row label={t('profile.updated')}>{new Date(profile.updated_ms).toLocaleString(i18n.language)}</Row>
      </CardContent>
    </Card>
  )
}

/** The game's 最近走勢: the last ranked games' places, per ladder. */
function TrendCard({ profile }: { profile: AccountProfile }) {
  const { t } = useTranslation()
  const ladders = ([3, 4] as const)
    .map((players) => ({ players, games: trendOf(profile.recent_games, players, TREND_GAMES) }))
    .filter((l) => l.games.length > 0)
  return (
    <Card>
      <CardHeader>
        <CardTitle className="text-sm uppercase tracking-wider">
          {t('profile.trend', { n: TREND_GAMES })}
        </CardTitle>
      </CardHeader>
      <CardContent className="grid gap-4">
        {ladders.length === 0 ? (
          <p className="text-sm text-muted-foreground">{t('profile.stats_empty')}</p>
        ) : (
          ladders.map((l) => <TrendLine key={l.players} players={l.players} games={l.games} />)
        )}
      </CardContent>
    </Card>
  )
}

/** A filled five-pointed star centred on (cx, cy): the big-win mark. */
function starPoints(cx: number, cy: number, r: number): string {
  return Array.from({ length: 10 }, (_, i) => {
    const rad = i % 2 === 0 ? r : r * 0.45
    const a = -Math.PI / 2 + (i * Math.PI) / 5
    return `${cx + rad * Math.cos(a)},${cy + rad * Math.sin(a)}`
  }).join(' ')
}

function TrendLine({ players, games }: { players: 3 | 4; games: { rank: number; big: boolean }[] }) {
  const { t } = useTranslation()
  const ranks = games.map((g) => g.rank)
  const W = 400
  const rowH = 22
  const H = rowH * (players - 1) + 16
  const x = (i: number) => 36 + (i * (W - 52)) / Math.max(1, ranks.length - 1)
  const y = (rank: number) => 8 + (rank - 1) * rowH
  return (
    <div>
      <div className="text-sm font-medium pb-1">{t(`profile.ladder_${players}`)}</div>
      <svg viewBox={`0 0 ${W} ${H}`} className="w-full" role="img" aria-label={t(`profile.ladder_${players}`)}>
        {Array.from({ length: players }, (_, i) => (
          <g key={i}>
            <line x1={36} x2={W - 16} y1={y(i + 1)} y2={y(i + 1)} className="stroke-border" strokeWidth={1} />
            <text x={0} y={y(i + 1) + 4} className="fill-muted-foreground text-[11px]">
              {t('profile.place', { n: i + 1 })}
            </text>
          </g>
        ))}
        <polyline
          points={ranks.map((r, i) => `${x(i)},${y(r)}`).join(' ')}
          fill="none"
          stroke="var(--color-chart-1)"
          strokeWidth={2}
        />
        {/* A big win (a first on more than twice the starting points) is a
            star, where the game draws its sunglasses cat. */}
        {games.map((g, i) =>
          g.big ? (
            <polygon key={i} points={starPoints(x(i), y(g.rank), 8)} fill="#facc15" stroke="#a16207" strokeWidth={0.8} />
          ) : (
            <circle key={i} cx={x(i)} cy={y(g.rank)} r={3.5} fill="var(--color-chart-1)" />
          ),
        )}
      </svg>
    </div>
  )
}

function StatsCard({ profile }: { profile: AccountProfile }) {
  const { t, i18n } = useTranslation()
  return (
    <Card>
      <CardHeader>
        <CardTitle className="text-sm uppercase tracking-wider">{t('profile.stats')}</CardTitle>
      </CardHeader>
      <CardContent className="grid gap-4">
        {profile.stats.length === 0 ? (
          <p className="text-sm text-muted-foreground">{t('profile.stats_empty')}</p>
        ) : (
          <div className="grid grid-cols-1 xl:grid-cols-2 gap-8">
            {[...profile.stats]
              .sort((a, b) => a.players - b.players)
              .map((s) => (
                <LadderStats key={s.players} stats={s} />
              ))}
          </div>
        )}
        <p className="text-xs text-muted-foreground text-right">
          {t('profile.stats_hint')}
          {profile.stats_at_ms != null &&
            ` ${t('profile.stats_at', { at: new Date(profile.stats_at_ms).toLocaleString(i18n.language) })}`}
        </p>
      </CardContent>
    </Card>
  )
}

function LadderStats({ stats }: { stats: MajsoulRankStats }) {
  const { t } = useTranslation()
  const games = stats.placements.reduce((a, b) => a + b, 0)
  const yakuman = stats.yakuman.reduce((a, [, n]) => a + n, 0)
  return (
    <div className="grid gap-3">
      <h3 className="font-medium">{t(`profile.ladder_${stats.players}`)}</h3>
      {stats.highest_hand && <HighestHandView hand={stats.highest_hand} />}
      <div className="grid gap-0.5">
        <Row label={t('profile.games')}>{games}</Row>
        {stats.placements.map((n, i) => (
          <Row key={i} label={t('profile.place', { n: i + 1 })}>
            {n} ({share(n, games)})
          </Row>
        ))}
        <Row label={t('profile.recent_ron', { n: stats.recent_rounds })}>
          {stats.recent_ron} ({share(stats.recent_ron, stats.recent_rounds)})
        </Row>
        <Row label={t('profile.recent_tsumo', { n: stats.recent_rounds })}>
          {stats.recent_tsumo} ({share(stats.recent_tsumo, stats.recent_rounds)})
        </Row>
        <Row label={t('profile.recent_deal_in', { n: stats.recent_rounds })}>
          {stats.recent_deal_in} ({share(stats.recent_deal_in, stats.recent_rounds)})
        </Row>
        <Row label={t('profile.average_win')}>{stats.average_win ?? '—'}</Row>
        <Row label={t('profile.yakuman')}>{yakuman}</Row>
      </div>
      <YakuList stats={stats} />
    </div>
  )
}

/** The game's 精選和牌: the concealed tiles, the calls and the winning tile. */
function HighestHandView({ hand }: { hand: HighestHand }) {
  const { t } = useTranslation()
  const rowRef = useRef<HTMLDivElement>(null)
  return (
    <div className="grid gap-1">
      <div className="flex items-baseline justify-between text-sm">
        <span className="text-muted-foreground">{t('profile.highest')}</span>
        <span className={cn('font-medium', isYakumanHan(hand.han) && 'text-red-400')}>
          {isYakumanHan(hand.han) ? t('profile.yakuman_hand') : t('profile.han', { n: hand.han })}
        </span>
      </div>
      <div ref={rowRef} className="flex flex-wrap items-end gap-2 leading-[0]">
        <Mahgen seq={mjaiToMahgen(hand.tiles)} kind="deal-in" containerRef={rowRef} className="leading-[0]" />
        {hand.melds.map((m, i) => (
          <Mahgen key={i} seq={mjaiToMahgen(m)} kind="deal-in" containerRef={rowRef} className="leading-[0]" />
        ))}
        {hand.win_tile && (
          <span className="pl-2">
            <Mahgen seq={mjaiToMahgen([hand.win_tile])} kind="deal-in" containerRef={rowRef} className="leading-[0]" />
          </span>
        )}
      </div>
    </div>
  )
}

/** Every yaku the game counts, in its own order (流局滿貫 before the
 *  yakuman, as its list puts it); the ones never won dimmed. */
function YakuList({ stats }: { stats: MajsoulRankStats }) {
  const { t } = useTranslation()
  const won = new Map(stats.yaku)
  const known = new Set(YAKUMAN_IDS)
  const entries: { key: string; label: string; n: number; yakuman: boolean }[] = [
    ...YAKU_IDS.map((id) => ({ key: `y${id}`, label: t(`profile.yaku_name.${id}`), n: won.get(id) ?? 0, yakuman: false })),
    { key: 'nagashi', label: t('profile.nagashi'), n: stats.nagashi, yakuman: false },
    ...YAKUMAN_IDS.map((id) => ({
      key: `y${id}`,
      label: known.has(id) ? t(`profile.yakuman_name.${id}`) : `#${id}`,
      n: won.get(id) ?? 0,
      yakuman: true,
    })),
  ]
  return (
    <div className="grid gap-1">
      <div className="text-sm text-muted-foreground">{t('profile.yaku_list')}</div>
      <div className="grid grid-cols-2 sm:grid-cols-3 gap-x-4 gap-y-0.5 text-xs">
        {entries.map((e) => (
          <div
            key={e.key}
            className={cn(
              'flex justify-between border-b border-border/30 py-0.5',
              e.n === 0 && 'text-muted-foreground/50',
              e.n > 0 && e.yakuman && 'text-red-400',
            )}
          >
            <span>{e.label}</span>
            <span className="font-mono">{e.n}</span>
          </div>
        ))}
      </div>
    </div>
  )
}

/** The game's own recent games when it has sent them, the history's
 *  otherwise; the model from the history either way, matched by uuid. */
function RecentCard({ profile }: { profile: AccountProfile }) {
  const { t, i18n } = useTranslation()
  const records = useHistoryStore((s) => s.records)
  const botByUuid = new Map<string, string>()
  for (const r of records) {
    const uuid = r.match_info && r.match_info.platform === 'majsoul' ? r.match_info.game_uuid : null
    if (uuid && r.bot) botByUuid.set(uuid, r.bot)
  }
  const botLabel = (bot: string | null | undefined) => (bot ? (nativeBotShortName(bot) ?? bot) : '—')
  const fromGame = profile.recent_games.slice(0, RECENT_GAMES)
  const fromHistory = recentGames(records, profile.account_id, RECENT_GAMES)

  const head = (
    <thead className="text-muted-foreground">
      <tr className="text-left">
        <th className="py-1 font-normal">{t('profile.col_time')}</th>
        <th className="py-1 font-normal">{t('profile.col_room')}</th>
        <th className="py-1 font-normal">{t('profile.col_rank')}</th>
        <th className="py-1 font-normal text-right">{t('profile.col_score')}</th>
        <th className="py-1 font-normal text-right">{t('profile.col_bot')}</th>
      </tr>
    </thead>
  )
  return (
    <Card>
      <CardHeader>
        <CardTitle className="text-sm uppercase tracking-wider">
          {t('profile.recent', { n: RECENT_GAMES })}
        </CardTitle>
      </CardHeader>
      <CardContent>
        {fromGame.length > 0 ? (
          <table className="w-full text-sm">
            {head}
            <tbody>
              {fromGame.map((g) => {
                const tier = majsoulRoomTier(g.mode_id)
                return (
                  <tr key={g.uuid || g.end_time} className="border-t border-border/40">
                    <td className="py-1">{new Date(g.start_time * 1000).toLocaleString(i18n.language)}</td>
                    <td className="py-1">
                      {g.ranked && tier ? t(`history.room.majsoul_${tier}`) : t('profile.not_ranked')}・
                      {t(`profile.players_${playersOfMode(g.mode_id)}`)}
                    </td>
                    <td className="py-1 font-mono">{g.rank}</td>
                    <td className="py-1 font-mono text-right">{g.final_point.toLocaleString(i18n.language)}</td>
                    <td className="py-1 text-right text-muted-foreground">{botLabel(botByUuid.get(g.uuid))}</td>
                  </tr>
                )
              })}
            </tbody>
          </table>
        ) : fromHistory.length > 0 ? (
          <table className="w-full text-sm">
            {head}
            <tbody>
              {fromHistory.map((g) => {
                const room = roomLabelKey(g.match_info)
                const score = g.our_seat != null ? g.final_scores[g.our_seat] : null
                return (
                  <tr key={g.id} className="border-t border-border/40">
                    <td className="py-1">{new Date(g.started_at).toLocaleString(i18n.language)}</td>
                    <td className="py-1">
                      {room ? t(room.key, room.params) : '—'}・{t(`profile.players_${g.num_players}`)}
                    </td>
                    <td className="py-1 font-mono">{g.our_rank ?? '—'}</td>
                    <td className="py-1 font-mono text-right">{score?.toLocaleString(i18n.language) ?? '—'}</td>
                    <td className="py-1 text-right text-muted-foreground">{botLabel(g.bot)}</td>
                  </tr>
                )
              })}
            </tbody>
          </table>
        ) : (
          <p className="text-sm text-muted-foreground">{t('profile.recent_empty')}</p>
        )}
      </CardContent>
    </Card>
  )
}
