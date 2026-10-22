// The icon of the main character the player has in the game (read off the
// login traffic — see `AccountProfile.main_character`). The pictures are in
// `public/characters/<id>.png`, made by `scripts/build_character_table.py`.

import { useState } from 'react'

import { characterIconUrl } from '@/lib/characters'
import { cn } from '@/lib/utils'

/** The character's round icon, or nothing when there is no such icon. */
export function CharacterIcon({
  id,
  className,
}: {
  id: number | null | undefined
  className?: string
}) {
  const [missing, setMissing] = useState<number | null>(null)
  if (id == null || missing === id) return null
  return (
    <img
      src={characterIconUrl(id)}
      alt=""
      aria-hidden
      draggable={false}
      onError={() => setMissing(id)}
      className={cn('shrink-0 rounded-full object-cover', className)}
    />
  )
}
