// Whether the player has seen the autoplay risk notice. Kept in the web view's
// storage, so it is asked once per account window.

const KEY = 'mokora.autoplayRiskAck'

export function autoplayRiskAcknowledged(): boolean {
  try {
    return localStorage.getItem(KEY) === '1'
  } catch {
    return false
  }
}

export function acknowledgeAutoplayRisk(): void {
  try {
    localStorage.setItem(KEY, '1')
  } catch {
    /* asked again next time */
  }
}
