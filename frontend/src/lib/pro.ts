// The paid module (`crate::pro`): downloaded once a key is redeemed, it holds
// the paid features. The interface only shows how its download goes.

/** Mirrors `crate::pro::ProStatus`. */
export type ProStatus =
  | { state: 'none' }
  | { state: 'downloading'; version: string; received: number; total: number }
  | { state: 'ready'; version: string }
  | { state: 'restart'; version: string }
  | { state: 'failed'; error: string }

/** Sent by the Rust side whenever the status changes. */
export const PRO_STATUS = 'pro-status'

/** Whole percent done, 0–100. */
export function percent(received: number, total: number): number {
  if (total <= 0) return 0
  return Math.min(100, Math.floor((100 * received) / total))
}

/** Bytes as "1.4 MB" (or "820 KB" under a megabyte). */
export function size(bytes: number): string {
  if (bytes < 1024 * 1024) return `${Math.max(1, Math.round(bytes / 1024))} KB`
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`
}

/** Whether the paid features can run: the module is in use. */
export function moduleReady(status: ProStatus | null): boolean {
  return status?.state === 'ready'
}
