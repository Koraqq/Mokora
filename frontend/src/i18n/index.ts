import i18n from 'i18next'
import { initReactI18next } from 'react-i18next'
import LanguageDetector from 'i18next-browser-languagedetector'

import { HAS_TAURI, invoke } from '@/lib/tauri'

import en from './resources/en.json'
import zhTW from './resources/zh-TW.json'
import zhCN from './resources/zh-CN.json'
import ja from './resources/ja.json'

export const SUPPORTED_LANGS = ['en', 'zh-TW', 'zh-CN', 'ja'] as const
export type SupportedLang = (typeof SUPPORTED_LANGS)[number]

export const LANG_LABELS: Record<SupportedLang, string> = {
  'en': 'English',
  'zh-TW': '繁體中文',
  'zh-CN': '简体中文',
  'ja': '日本語',
}

void i18n
  .use(LanguageDetector)
  .use(initReactI18next)
  .init({
    resources: {
      en:      { translation: en },
      'zh-TW': { translation: zhTW },
      'zh-CN': { translation: zhCN },
      ja:      { translation: ja },
    },
    fallbackLng: 'en',
    supportedLngs: SUPPORTED_LANGS,
    interpolation: { escapeValue: false },
    detection: {
      order: ['localStorage', 'navigator'],
      lookupLocalStorage: 'mokora.lang',
      caches: ['localStorage'],
    },
  })

// Accounts opened later start in the language chosen here (the backend keeps it
// for them; each account's interface has storage of its own).
if (HAS_TAURI) {
  const remember = (lng: string) => {
    invoke('set_ui_language', { lang: lng }).catch(() => {})
  }
  i18n.on('languageChanged', remember)
  // `init` may already have finished (the resources are bundled).
  if (i18n.language) remember(i18n.language)
}

export default i18n
