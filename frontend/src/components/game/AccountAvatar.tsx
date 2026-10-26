// The avatar at the top right of the play screen: the account this window
// plays, and a menu to switch to another, add one or delete one. It is the one
// place accounts are managed. Switching puts the other account's window where
// this one is (see `src/accounts/`).
//
// "Delete account" puts a delete button on every other account (the one on
// show cannot be deleted). Pressing one asks first; a box at the bottom left
// of that question chooses whether the account's game records go too. Kept
// records come back when that Mahjong Soul account logs in again.
//
// The menu opens under the avatar, inside the right-hand column — the game's
// web view paints above the page, so a menu reaching over the game would be
// hidden behind it.

import { useRef, useState } from 'react'
import { useTranslation } from 'react-i18next'
import { Check, ImagePlus, Loader2, Lock, Trash2, UserMinus, UserPlus } from 'lucide-react'
import { toast } from '@/components/ui/sonner'

import { Button } from '@/components/ui/button'
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog'
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu'
import { useAccounts } from '@/hooks/useAccounts'
import { useLicense } from '@/hooks/useLicense'
import { commandError, hasPro } from '@/lib/license'
import { CharacterIcon } from '@/components/game/CharacterIcon'
import {
  accountLabel,
  byRecentLogin,
  canAddAccount,
  type AccountInfo,
  type RemovedAccount,
} from '@/lib/accounts'
import { invoke } from '@/lib/tauri'
import { TONE_DOT } from '@/lib/statusIndicators'
import { cn } from '@/lib/utils'
import { useHistoryStore } from '@/stores/historyStore'
import type { GameRecord } from '@/types'

/** A stable hue per account, so each avatar is told apart at a glance. */
function hue(id: string): number {
  let h = 0
  for (const ch of id) h = (h * 31 + ch.charCodeAt(0)) % 360
  return h
}

/** The picture of `a`: the one the player chose, else their main character in
 *  the game, else the initial on a colour. */
function Face({ a, initial, size }: { a: AccountInfo; initial: string; size: string }) {
  if (a.avatar) {
    return <img src={a.avatar} alt="" className={cn(size, 'shrink-0 rounded-full object-cover')} />
  }
  return (
    <span
      className={cn(
        size,
        'relative flex shrink-0 items-center justify-center rounded-full font-semibold text-white',
      )}
      style={{ background: `hsl(${hue(a.id)} 55% 42%)` }}
    >
      {initial}
      {/* Over the initial; if there is no such icon the initial stays. */}
      <CharacterIcon id={a.character} className="absolute inset-0 h-full w-full" />
    </span>
  )
}

/** A picked image as a 128-pixel square PNG (base64), centred and cropped. */
async function squarePng(file: File): Promise<string> {
  const bitmap = await createImageBitmap(file)
  const side = Math.min(bitmap.width, bitmap.height)
  const canvas = document.createElement('canvas')
  canvas.width = canvas.height = 128
  const ctx = canvas.getContext('2d')
  if (!ctx) throw new Error('no canvas')
  ctx.drawImage(
    bitmap,
    (bitmap.width - side) / 2,
    (bitmap.height - side) / 2,
    side,
    side,
    0,
    0,
    128,
    128,
  )
  bitmap.close()
  return canvas.toDataURL('image/png').replace(/^data:image\/png;base64,/, '')
}

export function AccountAvatar() {
  const { t } = useTranslation()
  const { accounts, refresh } = useAccounts()
  const [license] = useLicense()
  // Unknown until the license answers: nothing is shown locked meanwhile.
  const addLocked = license !== null && !canAddAccount(accounts, hasPro(license))
  const [busy, setBusy] = useState<string | null>(null)
  const fileInput = useRef<HTMLInputElement>(null)
  const setRecords = useHistoryStore((st) => st.setRecords)
  // Delete mode: a delete button on every other account.
  const [deleting, setDeleting] = useState(false)
  const [asking, setAsking] = useState<AccountInfo | null>(null)
  const [clearRecords, setClearRecords] = useState(false)
  const [removing, setRemoving] = useState(false)

  const choosePicture = async (file: File | undefined) => {
    if (!file) return
    setBusy('avatar')
    try {
      await invoke('set_account_avatar', { pngBase64: await squarePng(file) })
    } catch (e) {
      toast.error(t('play.account.avatar_failed'), { description: String(e) })
    } finally {
      setBusy(null)
      refresh()
    }
  }

  const removePicture = async () => {
    try {
      await invoke('clear_account_avatar')
    } catch (e) {
      toast.error(String(e))
    } finally {
      refresh()
    }
  }

  const current = accounts.find((a) => a.current)
  const name = (a: AccountInfo) => accountLabel(a, t)
  const initial = (a: AccountInfo) => Array.from(name(a))[0]?.toUpperCase() ?? '?'
  const tone = (a: AccountInfo) => (a.online ? 'live' : a.running ? 'pending' : 'off')

  const pick = async (a: AccountInfo) => {
    if (a.current || busy) return
    if (a.locked) {
      toast.error(t('license.pro_accounts'))
      return
    }
    setBusy(a.id)
    try {
      await invoke('switch_account', { id: a.id })
    } catch (e) {
      toast.error(commandError(e, t))
    } finally {
      setBusy(null)
      refresh()
    }
  }

  const add = async () => {
    if (addLocked) {
      toast.error(t('license.pro_accounts'))
      return
    }
    setBusy('add')
    try {
      await invoke('add_account')
    } catch (e) {
      toast.error(commandError(e, t))
    } finally {
      setBusy(null)
      refresh()
    }
  }

  const askDelete = (a: AccountInfo) => {
    setClearRecords(false)
    setAsking(a)
  }

  const remove = async (a: AccountInfo) => {
    setRemoving(true)
    try {
      const done = await invoke<RemovedAccount>('delete_account', {
        id: a.id,
        clearRecords,
      })
      const label = name(a)
      toast.success(
        !clearRecords
          ? t('play.account.deleted_records_kept', { name: label })
          : done.records_kept
            ? t('sidebar.account.deleted_kept', { name: label })
            : t('sidebar.account.deleted', { name: label, n: done.records }),
      )
      if (done.left.length > 0) {
        toast.warning(t('sidebar.account.deleted_left'), { description: done.left.join('\n') })
      }
      if (clearRecords) {
        // The games that went: the pages showing them read the history again.
        invoke<GameRecord[]>('list_game_history', { filter: null, limit: 1_000_000, offset: 0 })
          .then(setRecords)
          .catch(() => {})
      }
    } catch (e) {
      toast.error(t('sidebar.account.delete_failed'), { description: String(e) })
    } finally {
      setRemoving(false)
      setAsking(null)
      refresh()
    }
  }

  if (!current) return null
  const others = accounts.filter((a) => !a.current)

  return (
    <>
    <input
      ref={fileInput}
      type="file"
      accept="image/*"
      className="hidden"
      onChange={(e) => {
        void choosePicture(e.target.files?.[0])
        e.target.value = ''
      }}
    />
    <DropdownMenu modal={false} onOpenChange={(o) => !o && setDeleting(false)}>
      <DropdownMenuTrigger asChild>
        <button
          type="button"
          title={name(current)}
          className="group relative flex h-8 w-8 items-center justify-center rounded-full text-xs outline-none ring-offset-background transition-transform duration-200 hover:scale-105 focus-visible:ring-2 focus-visible:ring-ring active:scale-95"
        >
          <Face a={current} initial={initial(current)} size="h-8 w-8" />
          <span
            className={cn(
              'absolute -bottom-0.5 -right-0.5 h-2.5 w-2.5 rounded-full border-2 border-background transition-colors duration-300',
              TONE_DOT[tone(current)],
            )}
          />
        </button>
      </DropdownMenuTrigger>
      <DropdownMenuContent
        align="end"
        className="w-64 duration-200 data-[state=closed]:animate-out data-[state=open]:animate-in data-[state=closed]:fade-out-0 data-[state=open]:fade-in-0 data-[state=open]:slide-in-from-top-2"
      >
        <DropdownMenuLabel className="text-xs text-muted-foreground">
          {t('play.account.title')}
        </DropdownMenuLabel>
        <div
          data-account-switch-list
          className="max-h-40 overflow-y-auto overscroll-contain scroll-smooth"
        >
          {byRecentLogin(accounts).map((a) => (
            <DropdownMenuItem
              key={a.id}
              onSelect={(e) => {
                // In delete mode a press asks to delete instead of switching.
                if (deleting) {
                  e.preventDefault()
                  if (!a.current) askDelete(a)
                  return
                }
                void pick(a)
              }}
              className="gap-2.5 transition-colors duration-150"
            >
              <Face a={a} initial={initial(a)} size="h-6 w-6 text-[11px]" />
              <span className="min-w-0 flex-1 truncate">{name(a)}</span>
              {deleting && !a.current ? (
                <Trash2
                  className="h-3.5 w-3.5 text-destructive"
                  aria-label={t('sidebar.account.delete_row')}
                />
              ) : busy === a.id ? (
                <Loader2 className="h-3.5 w-3.5 animate-spin" />
              ) : a.current ? (
                <Check className="h-3.5 w-3.5 text-primary" />
              ) : a.locked ? (
                <Lock
                  className="h-3.5 w-3.5 text-muted-foreground"
                  aria-label={t('license.pro_accounts')}
                />
              ) : (
                <span className={cn('h-1.5 w-1.5 rounded-full', TONE_DOT[tone(a)])} />
              )}
            </DropdownMenuItem>
          ))}
        </div>
        <DropdownMenuSeparator />
        <DropdownMenuItem onSelect={() => fileInput.current?.click()} className="gap-2.5">
          {busy === 'avatar' ? (
            <Loader2 className="h-4 w-4 animate-spin" />
          ) : (
            <ImagePlus className="h-4 w-4" />
          )}
          {t('play.account.change_avatar')}
        </DropdownMenuItem>
        {current.avatar && (
          <DropdownMenuItem onSelect={() => void removePicture()} className="gap-2.5">
            <Trash2 className="h-4 w-4" />
            {t('play.account.remove_avatar')}
          </DropdownMenuItem>
        )}
        <DropdownMenuItem
          onSelect={() => void add()}
          title={addLocked ? t('license.pro_accounts') : undefined}
          className={cn('gap-2.5', addLocked && 'opacity-60')}
        >
          {busy === 'add' ? (
            <Loader2 className="h-4 w-4 animate-spin" />
          ) : addLocked ? (
            <Lock className="h-4 w-4" />
          ) : (
            <UserPlus className="h-4 w-4" />
          )}
          {t('sidebar.account.add')}
        </DropdownMenuItem>
        <DropdownMenuItem
          disabled={others.length === 0}
          title={others.length === 0 ? t('play.account.delete_none') : undefined}
          className={cn('gap-2.5', deleting && 'text-destructive focus:text-destructive')}
          // The menu stays open: the delete buttons are what comes next.
          onSelect={(e) => {
            e.preventDefault()
            setDeleting((v) => !v)
          }}
        >
          <UserMinus className="h-4 w-4" />
          {deleting ? t('sidebar.account.delete_done') : t('sidebar.account.delete')}
        </DropdownMenuItem>
      </DropdownMenuContent>
    </DropdownMenu>
    <Dialog open={asking !== null} onOpenChange={(o) => !o && !removing && setAsking(null)}>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>
            {t('sidebar.account.delete_title', { name: asking ? name(asking) : '' })}
          </DialogTitle>
          <DialogDescription>
            {asking?.id === 'main'
              ? t('play.account.delete_body_main')
              : t('play.account.delete_body')}
          </DialogDescription>
        </DialogHeader>
        {asking?.running && (
          <p className="text-sm text-muted-foreground">{t('sidebar.account.delete_running')}</p>
        )}
        <DialogFooter className="mx-0 mb-0 flex-row items-center justify-between gap-2 border-0 bg-transparent p-0 sm:justify-between">
          <label className="flex cursor-pointer items-center gap-2 text-sm">
            <input
              type="checkbox"
              className="h-4 w-4 accent-destructive"
              checked={clearRecords}
              disabled={removing}
              onChange={(e) => setClearRecords(e.target.checked)}
            />
            {t('play.account.clear_records')}
          </label>
          <div className="flex items-center gap-2">
            <Button
              variant="outline"
              size="sm"
              disabled={removing}
              onClick={() => setAsking(null)}
            >
              {t('sidebar.account.delete_cancel')}
            </Button>
            <Button
              variant="destructive"
              size="sm"
              disabled={removing}
              onClick={() => asking && void remove(asking)}
            >
              {removing && <Loader2 className="h-4 w-4 animate-spin" />}
              {t('sidebar.account.delete_confirm')}
            </Button>
          </div>
        </DialogFooter>
      </DialogContent>
    </Dialog>
    </>
  )
}
