// Which operating system the app runs on, as its web view reports it: only
// Windows' WebView2 says "Windows" in its user agent, and only macOS'
// WKWebView "Macintosh" (Linux's WebKitGTK says "Linux").

const UA = typeof navigator !== 'undefined' ? navigator.userAgent : ''

export const IS_WINDOWS = /Windows/.test(UA)
export const IS_MAC = /Macintosh|Mac OS X/.test(UA)

/** Where a browser usually lives here, as an example path for an input. */
export const BROWSER_PATH_EXAMPLE = IS_WINDOWS
  ? 'C:\\Program Files\\Google\\Chrome\\Application\\chrome.exe'
  : '/usr/bin/google-chrome'
