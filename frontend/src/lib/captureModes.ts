// Which capture modes this build offers. The game in Mokora's own window
// (`embedded`) is read over DevTools from Windows' WebView2 and through
// WebKit from Linux's WebKitGTK; macOS' WKWebView offers neither yet, so
// there the launched browser (`chromium`) takes its place — the backend
// reads a config naming `embedded` as `chromium` there
// (`config::capture::CaptureMode`).

import { IS_MAC } from '@/lib/os'
import type { CaptureMode } from '@/types'

/** Whether the app's window can show and read the game. */
export const EMBEDDED_SUPPORTED = !IS_MAC

/** The mode a config without one gets, as the backend's default. */
export const DEFAULT_CAPTURE_MODE: CaptureMode = EMBEDDED_SUPPORTED ? 'embedded' : 'chromium'
