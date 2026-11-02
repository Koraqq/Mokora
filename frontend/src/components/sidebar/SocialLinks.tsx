// The project's links at the foot of the sidebar: round buttons in a row (a
// column when the sidebar is the narrow rail). Pointing at one lifts its
// button up and to the right, off a short stack of rings left behind it, and a
// label rises above — an isometric step rather than a plain hover.

import { DiscordMark } from '@/components/DiscordMark'
import { GithubMark } from '@/components/GithubMark'
import { MOKORA_DISCORD_URL, MOKORA_GITHUB_URL, openExternal } from '@/lib/external'
import { cn } from '@/lib/utils'

const LINKS = [
  { id: 'discord', label: 'Discord', url: MOKORA_DISCORD_URL, Mark: DiscordMark },
  { id: 'github', label: 'GitHub', url: MOKORA_GITHUB_URL, Mark: GithubMark },
]

const RING =
  'pointer-events-none absolute inset-0 rounded-full border border-primary opacity-0 transition-all duration-300 motion-reduce:transition-none'

export function SocialLinks({ open }: { open: boolean }) {
  return (
    <ul
      data-social-links
      className={cn(
        'flex shrink-0 gap-5 border-t border-border pt-5 pb-3',
        open ? 'flex-row items-center px-4' : 'flex-col items-center',
      )}
    >
      {LINKS.map(({ id, label, url, Mark }) => (
        <li key={id} className="group/social relative size-10">
          <span aria-hidden className={cn(RING, 'group-hover/social:opacity-20')} />
          <span
            aria-hidden
            className={cn(RING, 'group-hover/social:translate-x-1 group-hover/social:-translate-y-1 group-hover/social:opacity-40')}
          />
          <span
            aria-hidden
            className={cn(RING, 'group-hover/social:translate-x-2 group-hover/social:-translate-y-2 group-hover/social:opacity-60')}
          />
          <a
            href={url}
            aria-label={label}
            onClick={(e) => {
              e.preventDefault()
              openExternal(url)
            }}
            className={cn(
              'relative z-10 flex size-10 items-center justify-center rounded-full border border-border bg-muted/40 text-primary outline-none backdrop-blur',
              'shadow-[inset_0_0_12px_rgb(255_255_255/0.12),0_3px_3px_rgb(0_0_0/0.16)] transition-transform duration-300 motion-reduce:transition-none',
              'group-hover/social:-translate-y-3 group-hover/social:translate-x-3 focus-visible:-translate-y-3 focus-visible:translate-x-3 focus-visible:ring-2 focus-visible:ring-ring',
            )}
          >
            <Mark className="size-5" />
          </a>
          <span
            aria-hidden
            className={cn(
              'pointer-events-none absolute bottom-full left-1/2 z-20 mb-4 -translate-x-1/2 whitespace-nowrap rounded-md border border-border bg-popover px-2 py-1 text-xs text-popover-foreground opacity-0 shadow-md',
              'transition-all duration-300 motion-reduce:transition-none group-hover/social:-translate-y-1 group-hover/social:opacity-100',
            )}
          >
            {label}
          </span>
        </li>
      ))}
    </ul>
  )
}
