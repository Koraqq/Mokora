/**
 * Copy `text` to the system clipboard. Prefers the async Clipboard API
 * (available in WebView2 / WebKitGTK when invoked from a user gesture) and
 * falls back to the deprecated-but-ubiquitous `execCommand('copy')` path for
 * webviews that gate `navigator.clipboard` behind permissions Mokora doesn't
 * request. Returns whether a copy was actually performed, so callers can
 * toast success/failure honestly.
 */
export async function copyText(text: string): Promise<boolean> {
  try {
    if (navigator.clipboard?.writeText) {
      await navigator.clipboard.writeText(text)
      return true
    }
  } catch {
    /* fall through to execCommand */
  }
  try {
    const ta = document.createElement('textarea')
    ta.value = text
    // Off-screen, not display:none — a hidden textarea can't be selected.
    ta.style.position = 'fixed'
    ta.style.left = '-9999px'
    document.body.appendChild(ta)
    ta.select()
    const ok = document.execCommand('copy')
    ta.remove()
    return ok
  } catch {
    return false
  }
}
