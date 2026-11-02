// "分享與匯入" on the History page: hand one player's games to a friend by a
// code, or take a friend's by theirs.
//
//   分享  pick one of this computer's players → a code (good for three days).
//         The games are sealed on this computer; the server only keeps bytes
//         it cannot read, and the key is in the code alone.
//   匯入  paste a code → look it up → see what it holds → keep it. The player
//         then appears in the History picker as "name (分享 - N 場)". What is
//         kept is local from then on, and cannot be shared again.
//
// Only players recorded here can be shared: the list is this computer's own
// history, never a set received from a friend.

import { useEffect, useState } from 'react'
import { useTranslation } from 'react-i18next'
import { Copy, Loader2, Share2, Trash2 } from 'lucide-react'

import { toast } from '@/components/ui/sonner'
import { Button } from '@/components/ui/button'
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog'
import { Input } from '@/components/ui/input'
import { Label } from '@/components/ui/label'
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select'
import { Tabs, TabsContent, TabsList, TabsTrigger } from '@/components/ui/tabs'
import { recordsOfPlayer, type HistoryPlayer } from '@/lib/historyPlayers'
import { shareErrorKey } from '@/lib/historyShare'
import { invoke } from '@/lib/tauri'
import type { GameRecord, MadeShare, SharedSet, SharePreview } from '@/types'

type Props = {
  /** This computer's own players (never received ones). */
  players: HistoryPlayer[]
  records: GameRecord[]
  keys: Map<string, string>
  /** The player the page shows, opened on if it is one of `players`. */
  activeKey: string
  /** A set was kept: the page reads the received sets again and shows it. */
  onImported: (set: SharedSet) => void
}

const when = (seconds: number) => new Date(seconds * 1000).toLocaleString()
const day = (rfc3339: string) => (rfc3339 ? new Date(rfc3339).toLocaleDateString() : '—')

export function ShareDialog({ players, records, keys, activeKey, onImported }: Props) {
  const { t } = useTranslation()
  const [open, setOpen] = useState(false)
  const [tab, setTab] = useState<'share' | 'import'>('share')
  const [picked, setPicked] = useState<string | null>(null)
  const [made, setMade] = useState<MadeShare[]>([])
  const [fresh, setFresh] = useState<string | null>(null)
  const [code, setCode] = useState('')
  const [preview, setPreview] = useState<SharePreview | null>(null)
  const [busy, setBusy] = useState<'share' | 'lookup' | 'import' | null>(null)
  const [error, setError] = useState<string | null>(null)

  const playerKey =
    picked && players.some((p) => p.key === picked)
      ? picked
      : players.some((p) => p.key === activeKey)
        ? activeKey
        : (players[0]?.key ?? null)
  const player = players.find((p) => p.key === playerKey) ?? null
  const ids = player ? recordsOfPlayer(records, keys, player.key).map((r) => r.id) : []

  // The shares still running, read each time the window opens.
  useEffect(() => {
    if (!open) return
    let live = true
    invoke<MadeShare[]>('history_share_made')
      .then((list) => live && setMade(list))
      .catch(() => {})
    return () => {
      live = false
    }
  }, [open])

  const onOpenChange = (next: boolean) => {
    setOpen(next)
    if (!next) {
      setError(null)
      setFresh(null)
      setCode('')
      setPreview(null)
    }
  }

  const fail = (e: unknown, making = false) => setError(t(shareErrorKey(e, making)))

  const create = async () => {
    if (!player) return
    setBusy('share')
    setError(null)
    try {
      const share = await invoke<MadeShare>('history_share_create', {
        ids,
        name: player.name,
        player: player.key,
      })
      setMade((list) => [share, ...list.filter((s) => s.id !== share.id)])
      setFresh(share.id)
    } catch (e) {
      fail(e, true)
    } finally {
      setBusy(null)
    }
  }

  const copy = async (text: string) => {
    try {
      await navigator.clipboard.writeText(text)
      toast.success(t('history.share.copied'))
    } catch {
      toast.error(t('history.share.copy_failed'))
    }
  }

  const revoke = async (share: MadeShare) => {
    if (!window.confirm(t('history.share.revoke_confirm'))) return
    try {
      await invoke('history_share_revoke', { id: share.id })
      setMade((list) => list.filter((s) => s.id !== share.id))
      toast.success(t('history.share.revoked'))
    } catch (e) {
      fail(e)
    }
  }

  const lookup = async () => {
    setBusy('lookup')
    setError(null)
    setPreview(null)
    try {
      setPreview(await invoke<SharePreview>('history_import_lookup', { code: code.trim() }))
    } catch (e) {
      fail(e)
    } finally {
      setBusy(null)
    }
  }

  const keep = async () => {
    if (!preview) return
    setBusy('import')
    setError(null)
    try {
      const set = await invoke<SharedSet>('history_import_confirm', { id: preview.id })
      toast.success(t('history.share.imported', { name: set.name, count: set.records.length }))
      onImported(set)
      onOpenChange(false)
    } catch (e) {
      fail(e)
    } finally {
      setBusy(null)
    }
  }

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <Button variant="outline" onClick={() => onOpenChange(true)}>
        <Share2 className="h-4 w-4" />
        {t('history.share.button')}
      </Button>
      <DialogContent className="sm:max-w-lg">
        <DialogHeader>
          <DialogTitle>{t('history.share.title')}</DialogTitle>
          <DialogDescription>{t('history.share.description')}</DialogDescription>
        </DialogHeader>

        <Tabs
          value={tab}
          onValueChange={(v) => {
            setTab(v === 'import' ? 'import' : 'share')
            setError(null)
          }}
        >
          <TabsList className="w-full">
            <TabsTrigger value="share">{t('history.share.tab_share')}</TabsTrigger>
            <TabsTrigger value="import">{t('history.share.tab_import')}</TabsTrigger>
          </TabsList>

          <TabsContent value="share" className="mt-4 grid gap-4">
            {players.length === 0 ? (
              <p className="text-sm text-muted-foreground">{t('history.share.no_players')}</p>
            ) : (
              <>
                <div className="grid gap-2">
                  <Label>{t('history.share.player')}</Label>
                  <Select value={playerKey ?? undefined} onValueChange={setPicked}>
                    <SelectTrigger className="w-full" aria-label={t('history.share.player')}>
                      <SelectValue />
                    </SelectTrigger>
                    <SelectContent>
                      {players.map((p) => (
                        <SelectItem key={p.key} value={p.key}>
                          {t('history.filter.player_option', { name: p.name, count: p.games })}
                        </SelectItem>
                      ))}
                    </SelectContent>
                  </Select>
                </div>
                <Button onClick={() => void create()} disabled={busy !== null || ids.length === 0}>
                  {busy === 'share' && <Loader2 className="h-4 w-4 animate-spin" />}
                  {busy === 'share' ? t('history.share.making') : t('history.share.make')}
                </Button>
              </>
            )}

            {made.length > 0 && (
              <div className="grid gap-2">
                <Label>{t('history.share.made_title')}</Label>
                <ul className="grid gap-2">
                  {made.map((s) => (
                    <li
                      key={s.id}
                      data-share-id={s.id}
                      className={
                        'grid gap-1.5 rounded-md border p-3 ' +
                        (s.id === fresh ? 'border-primary' : 'border-border')
                      }
                    >
                      <div className="flex items-center justify-between gap-2 text-sm">
                        <span className="min-w-0 truncate font-medium">
                          {t('history.filter.player_option', { name: s.name, count: s.games })}
                        </span>
                        <Button
                          variant="ghost"
                          size="sm"
                          onClick={() => void revoke(s)}
                          aria-label={t('history.share.revoke')}
                          title={t('history.share.revoke')}
                        >
                          <Trash2 className="h-4 w-4" />
                        </Button>
                      </div>
                      <div className="flex items-center gap-2">
                        <Input readOnly value={s.code} className="font-mono text-xs" aria-label={t('history.share.code')} />
                        <Button
                          variant="outline"
                          size="icon"
                          onClick={() => void copy(s.code)}
                          aria-label={t('history.share.copy')}
                          title={t('history.share.copy')}
                        >
                          <Copy className="h-4 w-4" />
                        </Button>
                      </div>
                      <p className="text-xs text-muted-foreground">
                        {t('history.share.expires', { time: when(s.expires_at) })}
                        {s.id === fresh && s.free_left != null && (
                          <> · {t('history.share.free_left', { count: s.free_left })}</>
                        )}
                      </p>
                    </li>
                  ))}
                </ul>
              </div>
            )}
          </TabsContent>

          <TabsContent value="import" className="mt-4 grid gap-4">
            <div className="grid gap-2">
              <Label htmlFor="share-code">{t('history.share.import_hint')}</Label>
              <div className="flex gap-2">
                <Input
                  id="share-code"
                  value={code}
                  onChange={(e) => {
                    setCode(e.target.value)
                    setPreview(null)
                    setError(null)
                  }}
                  onKeyDown={(e) => e.key === 'Enter' && code.trim() && void lookup()}
                  placeholder="MK-XXXX-XXXX-…"
                  className="font-mono text-xs"
                  autoComplete="off"
                  spellCheck={false}
                />
                <Button onClick={() => void lookup()} disabled={busy !== null || !code.trim()}>
                  {busy === 'lookup' && <Loader2 className="h-4 w-4 animate-spin" />}
                  {busy === 'lookup' ? t('history.share.looking') : t('history.share.lookup')}
                </Button>
              </div>
            </div>

            {preview && (
              <div data-share-preview className="grid gap-3 rounded-md border border-border p-3">
                <div>
                  <div className="text-xs text-muted-foreground">{t('history.share.found')}</div>
                  <div className="text-base font-medium">
                    {t('history.filter.player_option_shared', {
                      name: preview.name,
                      count: preview.games,
                    })}
                  </div>
                  <div className="text-xs text-muted-foreground">
                    {t('history.share.range', { first: day(preview.first), last: day(preview.last) })}
                  </div>
                </div>
                {preview.replaces != null && (
                  <p className="text-xs text-amber-500">
                    {t('history.share.replaces', { count: preview.replaces })}
                  </p>
                )}
                <p className="text-xs text-muted-foreground">{t('history.share.no_reshare')}</p>
                <Button onClick={() => void keep()} disabled={busy !== null}>
                  {busy === 'import' && <Loader2 className="h-4 w-4 animate-spin" />}
                  {busy === 'import' ? t('history.share.importing') : t('history.share.import')}
                </Button>
              </div>
            )}
          </TabsContent>
        </Tabs>

        {error && (
          <p role="alert" className="text-sm text-destructive">
            {error}
          </p>
        )}
      </DialogContent>
    </Dialog>
  )
}
