import { invoke } from '@/lib/tauri'

export const MOKORA_DISCORD_URL = 'https://discord.gg/Ta6bqH6NwN'
// The project's home, where releases are published (the updater asks this repository).
export const MOKORA_GITHUB_URL = 'https://github.com/Koraqq/Mokora'

// Tauri 2's webview doesn't reliably honour `<a target="_blank">`
// without the opener plugin, so route external links through the
// `open_external_url` backend command, which hands the URL to the OS
// default-browser handler (ShellExecuteW / open / xdg-open).
export function openExternal(url: string): void {
  invoke('open_external_url', { url }).catch(() => {
    /* surfaced via Sonner toast hooked into the tauri error bridge */
  })
}
