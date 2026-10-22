import icon from '@/assets/logo/mokora-icon.svg'
import logoLight from '@/assets/logo/mokora-logo-light.svg'
import logoDark from '@/assets/logo/mokora-logo-dark.svg'
import { cn } from '@/lib/utils'

// The tile mark is one inking (ivory face, green rim) that reads on either
// background; only the wordmark's letters change. The theme store toggles the
// `.dark` class on <html>, so we render both wordmarks and let Tailwind's
// `dark:` variant pick — this tracks light/dark/system without subscribing to
// the store. Regenerate every asset with `python scripts/gen_icons.py`.

/** The mahjong-tile mark (no wordmark). Portrait ratio ≈ 0.78. */
export function MokoraIcon({ className }: { className?: string }) {
  return (
    <span className={cn('inline-flex shrink-0', className)}>
      <img src={icon} alt="" aria-hidden className="h-full w-auto" />
    </span>
  )
}

/** The full logo: tile mark + "Mokora" wordmark. */
export function MokoraWordmark({ className }: { className?: string }) {
  return (
    <span className={cn('inline-flex shrink-0', className)}>
      <img src={logoLight} alt="Mokora" className="h-full w-auto dark:hidden" />
      <img src={logoDark} alt="Mokora" className="hidden h-full w-auto dark:block" />
    </span>
  )
}
