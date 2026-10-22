# Sidebar

The app shell's navigation. It has **two layouts**, chosen by viewport width.

| Width | Layout | State that drives it |
| --- | --- | --- |
| `>= 1024px` (Tailwind `lg`) | Docked rail, 18rem pinned / 5.625rem collapsed | `isOpen` (pinned), `isHover` (peek) |
| `< 1024px` | Off-screen overlay drawer + backdrop | `isDrawerOpen` |

## Files

- `Sidebar.tsx` — the `<aside>` itself, plus the drawer backdrop. Renders both layouts.
- `NarrowTopBar.tsx` — the hamburger trigger. Returns `null` when docked; it is the **only** way to open the drawer, so don't let it become unreachable.
- `Menu.tsx` — the nav list, shared by both layouts. Items come from `@/lib/menuList`.

## Why narrow mode is driven from JS, not just `lg:` classes

The sidebar used to be hidden below `lg` purely with `-translate-x-full lg:translate-x-0`.
That slid it off-screen while leaving its only reveal controls — the pin button and the
hover handlers — *inside* the off-screen element, so the app had no navigation at all
between the window's minimum width and 1024px.

`useIsNarrow()` (`@/hooks/useIsNarrow`) now exposes that breakpoint to React via
`matchMedia`, which lets the drawer do things CSS can't express:

- close on navigation (a drawer that survives a route change covers the new page),
- close on <kbd>Esc</kbd> and on backdrop click,
- close when the window grows back past `lg`, so the backdrop can't linger,
- mark the off-screen `<aside>` `inert`, keeping its links out of the Tab order.

`LG_BREAKPOINT_PX` must stay equal to Tailwind's `lg` (1024). `App.tsx` still uses the
`lg:ml-*` classes for the docked content margin, so if the constant and the utility
disagree, the sidebar and the content margin disagree about who owns the layout.
A test in `useIsNarrow.test.ts` pins the value.

## Extending

- **New nav item** — add it to `getMenuList()` in `@/lib/menuList`; both layouts pick it up.
- **New control in the sidebar header** — remember the header renders a *close* (X) button in
  drawer mode and the *pin* (chevron) button when docked. Branch on `isNarrow`.
- **Anything that can hide navigation** — add a case to `NarrowTopBar.test.tsx`. The drawer
  is only usable if its trigger is reachable; that is the regression this module exists to
  prevent (issue #185).

## Drawer state is deliberately not persisted

`useSidebar` (`@/hooks/useSidebar`) persists to `localStorage` under `mokora.sidebar`, but
`partialize` keeps only `isOpen` and `settings`. `isDrawerOpen` and `isHover` are transient
view state — persisting the drawer would reopen it, backdrop and all, on every launch.
