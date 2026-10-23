import { useEffect } from 'react'
import { Outlet, useLocation } from 'react-router-dom'
import { Sidebar } from '@/components/sidebar/Sidebar'
import { NarrowTopBar } from '@/components/sidebar/NarrowTopBar'
import { Statusbar } from '@/components/Statusbar'
import { ErrorBoundary } from '@/components/ErrorBoundary'
import { Toaster } from '@/components/ui/sonner'
import { UpdateNotifier } from '@/components/UpdateNotifier'
import { ProModuleNotifier } from '@/components/ProModuleNotifier'
import { LicenseLossNotifier } from '@/components/LicenseLossNotifier'
import { AnnouncementsDialog } from '@/components/AnnouncementsDialog'
import { InstallBlockingOverlay } from '@/components/InstallBlockingOverlay'
import { SettingsLock } from '@/components/SettingsLock'
import { NoCoinsDialog } from '@/components/NoCoinsDialog'
import { QuitDialog } from '@/components/QuitDialog'
import { useEscapeToHome } from '@/hooks/useEscapeToHome'
import { useTauriBridge } from '@/hooks/useTauriBridge'
import { useSidebar } from '@/hooks/useSidebar'
import { useUiPrefsStore } from '@/stores/uiPrefsStore'
import { cn } from '@/lib/utils'

export default function App() {
  useTauriBridge()
  useEscapeToHome()
  const location = useLocation()
  const scale = useUiPrefsStore((s) => s.scale)
  const isOpen = useSidebar((s) => s.isOpen)
  const settings = useSidebar((s) => s.settings)

  // Apply UI scale by adjusting :root font-size — the design system is
  // rem-based so this scales typography, spacing, icons, sidebar width
  // (w-[18rem] / w-[5.625rem]), and Radix portals uniformly.
  useEffect(() => {
    document.documentElement.style.fontSize = `${16 * scale}px`
    return () => {
      document.documentElement.style.fontSize = ''
    }
  }, [scale])

  // The play screen is the whole window: no sidebar, no status bar of this shell.
  if (location.pathname === '/play') {
    return (
      <>
        <ErrorBoundary>
          <Outlet />
        </ErrorBoundary>
        <Toaster compact />
        <UpdateNotifier />
        <ProModuleNotifier />
        <LicenseLossNotifier />
        <AnnouncementsDialog />
        <InstallBlockingOverlay />
        <NoCoinsDialog />
        <QuitDialog />
      </>
    )
  }

  return (
    <>
      {/* Unsaved settings lock everything but the settings page itself. */}
      <SettingsLock>
        <Sidebar />
      </SettingsLock>
      <main
        className={cn(
          'h-screen flex flex-col min-w-0 overflow-hidden bg-background text-foreground transition-[margin-left] ease-in-out duration-300',
          // Margin tracks the *pinned* width only — hover-open expands the
          // sidebar over main content (z-20) without shifting layout, so
          // width-sensitive widgets like the GameDashboard's react-grid-layout
          // don't re-flow every time the cursor brushes the sidebar.
          settings.disabled
            ? 'lg:ml-0'
            : isOpen
              ? 'lg:ml-[18rem]'
              : 'lg:ml-[5.625rem]',
        )}
      >
        {/* Below `lg` the sidebar is an off-screen drawer, so this is the
            only navigation entry point. Renders null when docked. */}
        <SettingsLock>
          <NarrowTopBar />
        </SettingsLock>
        <div className="flex-1 overflow-auto">
          {/* Keyed by route so a crash on one page doesn't poison the others. */}
          <ErrorBoundary key={location.pathname}>
            {/* Each page fades in. */}
            <div className="animate-in fade-in duration-300">
              <Outlet />
            </div>
          </ErrorBoundary>
        </div>
        <SettingsLock>
          <Statusbar />
        </SettingsLock>
      </main>
      <Toaster />
      <UpdateNotifier />
      <ProModuleNotifier />
      <LicenseLossNotifier />
      <AnnouncementsDialog />
      <InstallBlockingOverlay />
      <NoCoinsDialog />
      <QuitDialog />
    </>
  )
}
