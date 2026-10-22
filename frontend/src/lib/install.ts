import { useInstallStore } from '@/stores/installStore'

// Run a long-running bot env install/sync while the global blocking overlay
// is shown. The overlay (see InstallBlockingOverlay) covers the whole app
// including the sidebar, so the user can't navigate away or start a game
// until the environment finishes installing — otherwise the bot's first
// in-game spawn would run `uv sync` mid-match and blow the react time limit.
export async function withInstallBlock<T>(fn: () => Promise<T>): Promise<T> {
  const { begin, end } = useInstallStore.getState()
  begin()
  try {
    return await fn()
  } finally {
    end()
  }
}
