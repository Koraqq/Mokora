// Game-history page. Player picker + 3p/4p toggle in the header (players
// from `lib/historyPlayers`); filter bar + PT-rule selector at the top; rank
// pie + cumulative-PT line chart in the middle; detailed stats and the
// game list at the bottom.
//
// The PT-rule card's header holds the view every chart and stats card
// follows — one day or all, friendly / event switches, and which bot's games
// — so one press changes them all. The game list keeps its own day picker.
//
// All filtering happens in-memory off the records cached in
// `useHistoryStore`. The store is hydrated by `useTauriBridge` on
// startup and kept current by the `history-recorded` Tauri event.

import { useEffect, useMemo, useState } from 'react'
import { useTranslation } from 'react-i18next'
import { invoke } from '@tauri-apps/api/core'
import { Trash2 } from 'lucide-react'
import { toast } from '@/components/ui/sonner'
import { listen } from '@/lib/tauri'

import {
  Card,
  CardContent,
  CardHeader,
  CardTitle,
} from '@/components/ui/card'
import { Input } from '@/components/ui/input'
import { Label } from '@/components/ui/label'
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select'
import { Button } from '@/components/ui/button'
import { Tabs, TabsList, TabsTrigger } from '@/components/ui/tabs'
import { BotTabs } from '@/components/history/BotTabs'
import { CumulativePtChart } from '@/components/history/CumulativePtChart'
import { DayPicker } from '@/components/history/DayPicker'
import { DealInTiles } from '@/components/history/DealInTiles'
import { KindToggles } from '@/components/history/KindToggles'
import { ShareDialog } from '@/components/history/ShareDialog'
import { GameList } from '@/components/history/GameList'
import { PtRuleSelector } from '@/components/history/PtRuleSelector'
import { RankPieChart } from '@/components/history/RankPieChart'
import { StatsTable } from '@/components/history/StatsTable'
import { useAgainstStats } from '@/hooks/useAgainstStats'
import { fetchAccounts, ownPlayerKey } from '@/lib/accounts'
import { recordsOfBot, type BotView } from '@/lib/historyBots'
import { gameDays, recordsOfDay } from '@/lib/historyDays'
import { listPlayers, playerKeys, recordsOfPlayer } from '@/lib/historyPlayers'
import { isSharedKey, recordsOfShared, sharedKey, sharedPlayers } from '@/lib/historyShare'
import { RANKED_ONLY, recordsOfKinds, type KindView } from '@/lib/historyKinds'
import { aggregateStats } from '@/lib/historyStats'
import {
  cumulativePtSeries,
  rankDistribution,
  withCurrentRank,
} from '@/lib/ptCalc'
import { ALL_PLAYERS, useHistoryStore } from '@/stores/historyStore'
import type { GameRecord, KyokuMode, Platform, SharedSet } from '@/types'

const ANY = '__any__'

export function History() {
  const { t } = useTranslation()
  const records = useHistoryStore((s) => s.records)
  const filter = useHistoryStore((s) => s.filter)
  const setFilter = useHistoryStore((s) => s.setFilter)
  const rule = useHistoryStore((s) => s.rule)
  const player = useHistoryStore((s) => s.player)
  const setPlayer = useHistoryStore((s) => s.setPlayer)
  const setRecords = useHistoryStore((s) => s.setRecords)
  // The players received from friends: kept apart from the history, listed in
  // the picker, in nobody's "all players".
  const [shared, setShared] = useState<SharedSet[]>([])

  // The store is shared by every account's Mokora (see `src/accounts/`), and
  // only this one's own games arrive as events — so re-read it whenever the
  // page is opened or the window comes back to the front.
  useEffect(() => {
    const reload = () => {
      invoke<GameRecord[]>('list_game_history', {
        filter: null,
        limit: 1_000_000,
        offset: 0,
      })
        .then(setRecords)
        .catch(() => {
          /* keep what is loaded */
        })
      invoke<SharedSet[]>('history_shared_list')
        .then(setShared)
        .catch(() => {
          /* keep what is loaded */
        })
    }
    reload()
    window.addEventListener('focus', reload)
    return () => window.removeEventListener('focus', reload)
  }, [setRecords])

  // Who this Mokora is logged in as — the player the page opens on. Re-read
  // when the login changes (the first login of a new account lands here).
  const [ownPlayer, setOwnPlayer] = useState<string | null>(null)
  useEffect(() => {
    const load = () => {
      fetchAccounts()
        .then((accounts) => setOwnPlayer(ownPlayerKey(accounts)))
        .catch(() => {
          /* fall back to whoever played last */
        })
    }
    load()
    let unlisten: (() => void) | undefined
    listen('accounts-changed', load).then((u) => {
      unlisten = u
    })
    return () => unlisten?.()
  }, [])

  // 3p / 4p are separate analysis modes — never mix. Defaults to 4p.
  const numPlayers: 3 | 4 = filter.num_players === 3 ? 3 : 4
  const setNumPlayers = (np: 3 | 4) => setFilter({ ...filter, num_players: np })

  // Whose games: across both modes, so switching 3p / 4p keeps the player.
  // Unset (or a player whose games were all deleted) means this Mokora's own
  // login, or — before it has a game, or a login — the one who played last.
  const keys = useMemo(() => playerKeys(records), [records])
  const ownPlayers = useMemo(() => listPlayers(records, keys), [records, keys])
  const players = useMemo(
    () => [...ownPlayers, ...sharedPlayers(shared)],
    [ownPlayers, shared],
  )
  const activePlayer =
    player === ALL_PLAYERS || players.some((p) => p.key === player)
      ? (player as string)
      : players.some((p) => p.key === ownPlayer)
        ? (ownPlayer as string)
        : (players[0]?.key ?? ALL_PLAYERS)

  const activeShared = isSharedKey(activePlayer)
  // The active player's games: a received set's, or this computer's own.
  const ofPlayer = useMemo(
    () =>
      activeShared
        ? recordsOfShared(shared, activePlayer)
        : recordsOfPlayer(records, keys, activePlayer),
    [activeShared, shared, records, keys, activePlayer],
  )

  const filtered = useMemo(
    () => ofPlayer.filter((r) => matchFilter(r, filter)),
    [ofPlayer, filter],
  )

  // "Current" room / rank for an auto PT rule: the player's newest game in
  // this mode, whatever the other filters narrow the page to.
  const ptRule = useMemo(
    () =>
      withCurrentRank(
        rule,
        ofPlayer.filter((r) => r.num_players === numPlayers),
      ),
    [rule, ofPlayer, numPlayers],
  )

  // The charts' and stats' view, set in the PT-rule card: which bot's games
  // ("all" is the combined view the page always had), friendly rooms and
  // events (neither earns rank points, so out unless switched on), and one
  // day or every day. A bot's — or a day's — cumulative PT counts from its
  // own first game.
  const [viewBot, setViewBot] = useState<BotView>('all')
  const [viewKinds, setViewKinds] = useState<KindView>(RANKED_ONLY)
  const [pickedDay, setPickedDay] = useState<string | null>(null)

  const ofView = useMemo(
    () => recordsOfBot(recordsOfKinds(filtered, viewKinds), viewBot),
    [filtered, viewKinds, viewBot],
  )
  const days = useMemo(() => gameDays(ofView), [ofView])
  // A day the other switches no longer leave falls back to every day.
  const day = days.some((d) => d.day === pickedDay) ? pickedDay : null
  const shown = useMemo(() => recordsOfDay(ofView, day), [ofView, day])

  const series = useMemo(() => cumulativePtSeries(shown, ptRule), [shown, ptRule])
  const stats = useMemo(() => aggregateStats(shown), [shown])
  const against = useAgainstStats(shown)
  const pieCounts = useMemo(() => rankDistribution(shown, numPlayers), [shown, numPlayers])

  return (
    <div className="p-6 flex flex-col gap-6 w-full">
      <header className="flex items-center justify-between gap-4 flex-wrap">
        <div>
          <h1 className="text-2xl font-semibold">{t('history.title')}</h1>
          <p className="text-sm text-muted-foreground">
            {t('history.description')}
          </p>
        </div>
        <div className="flex items-center gap-2">
          <ShareDialog
            players={ownPlayers}
            records={records}
            keys={keys}
            activeKey={activePlayer}
            onImported={(set) => {
              setShared((list) => [set, ...list.filter((s) => s.id !== set.id)])
              setPlayer(sharedKey(set.id))
            }}
          />
          {players.length > 0 && (
            <Select value={activePlayer} onValueChange={setPlayer}>
              <SelectTrigger
                className="w-48"
                aria-label={t('history.filter.player')}
                title={t('history.filter.player')}
              >
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                {players.map((p) => (
                  <SelectItem key={p.key} value={p.key}>
                    {t(
                      p.shared
                        ? 'history.filter.player_option_shared'
                        : 'history.filter.player_option',
                      { name: p.name, count: p.games },
                    )}
                  </SelectItem>
                ))}
                <SelectItem value={ALL_PLAYERS}>
                  {t('history.filter.all_players')}
                </SelectItem>
              </SelectContent>
            </Select>
          )}
          {activeShared && (
            <Button
              variant="ghost"
              size="icon"
              aria-label={t('history.share.remove')}
              title={t('history.share.remove')}
              onClick={() => {
                const set = shared.find((s) => sharedKey(s.id) === activePlayer)
                if (!set || !window.confirm(t('history.share.remove_confirm', { name: set.name }))) return
                invoke('history_shared_remove', { id: set.id })
                  .then(() => {
                    setShared((list) => list.filter((s) => s.id !== set.id))
                    toast.success(t('history.share.removed'))
                  })
                  .catch((e) => toast.error(String(e)))
              }}
            >
              <Trash2 className="h-4 w-4" />
            </Button>
          )}
          <Tabs
            value={String(numPlayers)}
            onValueChange={(v) => setNumPlayers(v === '3' ? 3 : 4)}
          >
            <TabsList>
              <TabsTrigger value="4">
                {t('history.filter.num_players_4')}
              </TabsTrigger>
              <TabsTrigger value="3">
                {t('history.filter.num_players_3')}
              </TabsTrigger>
            </TabsList>
          </Tabs>
        </div>
      </header>

      <div className="grid grid-cols-1 lg:grid-cols-3 gap-4">
        <FilterCard
          filter={filter}
          onChange={setFilter}
        />
        <div className="lg:col-span-2">
          <PtRuleSelector
            current={ptRule}
            action={
              <div
                className="flex items-center gap-2 flex-wrap"
                title={t('history.view.hint')}
              >
                <DayPicker days={days} value={day} onChange={setPickedDay} />
                <KindToggles value={viewKinds} onChange={setViewKinds} />
                <BotTabs value={viewBot} onChange={setViewBot} />
              </div>
            }
          />
        </div>
      </div>

      <div className="grid grid-cols-1 lg:grid-cols-3 gap-4">
        <Card>
          <CardHeader>
            <CardTitle className="text-sm uppercase tracking-wider">
              {t('history.rank_distribution')}
            </CardTitle>
          </CardHeader>
          <CardContent>
            <RankPieChart counts={pieCounts} />
          </CardContent>
        </Card>
        <Card className="lg:col-span-2">
          <CardHeader>
            <CardTitle className="text-sm uppercase tracking-wider">
              {t('history.cumulative_pt')}
            </CardTitle>
          </CardHeader>
          <CardContent>
            <CumulativePtChart series={series} />
          </CardContent>
        </Card>
      </div>

      <StatsTable stats={stats} against={against} numPlayers={numPlayers} />

      <DealInTiles records={shown} />

      <GameList records={filtered} rule={ptRule} readOnly={activeShared} />
    </div>
  )
}

function matchFilter(
  r: GameRecord,
  f: ReturnType<typeof useHistoryStore.getState>['filter'],
): boolean {
  if (f.platform && r.platform !== f.platform) return false
  if (f.num_players && r.num_players !== f.num_players) return false
  if (f.kyoku_mode && r.kyoku_mode !== f.kyoku_mode) return false
  if (f.started_after && new Date(r.started_at) < new Date(f.started_after))
    return false
  if (f.started_before && new Date(r.started_at) >= new Date(f.started_before))
    return false
  return true
}

function FilterCard({
  filter,
  onChange,
}: {
  filter: ReturnType<typeof useHistoryStore.getState>['filter']
  onChange: (f: ReturnType<typeof useHistoryStore.getState>['filter']) => void
}) {
  const { t } = useTranslation()

  return (
    <Card>
      <CardHeader>
        <CardTitle className="text-sm uppercase tracking-wider">{t('history.filters_card_title')}</CardTitle>
      </CardHeader>
      <CardContent>
        <div className="grid grid-cols-1 md:grid-cols-2 gap-3">
          <Field label={t('history.filter.platform')}>
            <Select
              value={filter.platform ?? ANY}
              onValueChange={(v) =>
                onChange({
                  ...filter,
                  platform: v === ANY ? undefined : (v as Platform),
                })
              }
            >
              <SelectTrigger className="w-full">
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value={ANY}>{t('history.filter.any')}</SelectItem>
                <SelectItem value="majsoul">{t('platform.majsoul')}</SelectItem>
                <SelectItem value="mjai">{t('platform.mjai')}</SelectItem>
              </SelectContent>
            </Select>
          </Field>

          <Field label={t('history.filter.kyoku_mode')}>
            <Select
              value={filter.kyoku_mode ?? ANY}
              onValueChange={(v) =>
                onChange({
                  ...filter,
                  kyoku_mode: v === ANY ? undefined : (v as KyokuMode),
                })
              }
            >
              <SelectTrigger className="w-full">
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value={ANY}>{t('history.filter.any')}</SelectItem>
                <SelectItem value="east_only">{t('history.filter.east_only')}</SelectItem>
                <SelectItem value="east_south">{t('history.filter.east_south')}</SelectItem>
                <SelectItem value="other">{t('history.filter.other')}</SelectItem>
              </SelectContent>
            </Select>
          </Field>

          <Field label={t('history.date_label')}>
            <div className="flex gap-1">
              <Input
                type="date"
                value={dateOnly(filter.started_after)}
                onChange={(e) =>
                  onChange({
                    ...filter,
                    started_after: e.target.value
                      ? new Date(e.target.value).toISOString()
                      : undefined,
                  })
                }
              />
              <Input
                type="date"
                value={dateOnly(filter.started_before)}
                onChange={(e) =>
                  onChange({
                    ...filter,
                    started_before: e.target.value
                      ? new Date(e.target.value).toISOString()
                      : undefined,
                  })
                }
              />
            </div>
          </Field>
        </div>
      </CardContent>
    </Card>
  )
}

function Field({ label, children }: { label: string; children: React.ReactNode }) {
  return (
    <div className="space-y-1">
      <Label className="text-xs">{label}</Label>
      {children}
    </div>
  )
}

function dateOnly(iso?: string): string {
  if (!iso) return ''
  const d = new Date(iso)
  if (Number.isNaN(d.valueOf())) return ''
  return d.toISOString().slice(0, 10)
}
