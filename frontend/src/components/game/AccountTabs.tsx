// The accounts that are open, as tabs beside the automation switch: one framed
// chip each — "name  in the lobby" with a green light, "name  in a game" with a
// red one, or "name  logging in" with a yellow one. Pressing a chip switches
// the window to that account; dragging it reorders all account windows; the
// fixed tail button confirms before closing that account (see `src/accounts/`).
// An account that has not been opened in this run has no chip.
//
// Three chips show at a time. With more, an arrow at the right slides the row
// along one chip (and one at the left slides back).

import { useEffect, useLayoutEffect, useMemo, useRef, useState } from "react";
import { flushSync } from "react-dom";
import { useTranslation } from "react-i18next";
import { ChevronLeft, ChevronRight, Loader2, LogOut } from "lucide-react";
import { toast } from "sonner";

import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { CharacterIcon } from "@/components/game/CharacterIcon";
import { useAccounts } from "@/hooks/useAccounts";
import { accountLabel, swapAccountIds, type AccountInfo } from "@/lib/accounts";
import { invoke, listen } from "@/lib/tauri";
import { cn } from "@/lib/utils";

const VISIBLE = 3;
/** A deliberate hold separates reordering from the account button's normal click. */
const LONG_PRESS_MS = 250;
const MOVE_TOLERANCE_PX = 5;
const VIEWPORT_SYNC_EVENT = "mokora-account-tabs-sync";
const VIEWPORT_TRANSITION_MS = 300;
/** One chip's width plus the gap after it, in rem — the row slides by this. */
const CHIP_REM = 12;
const GAP_REM = 0.5;

const injectedFirst = () => {
  const value = Number(
    (window as typeof window & { __MOKORA_ACCOUNT_TABS_FIRST__?: number })
      .__MOKORA_ACCOUNT_TABS_FIRST__,
  );
  return Number.isInteger(value) && value >= 0 ? value : 0;
};

type Phase = "in_game" | "lobby" | "connecting";
type DragVisual = {
  accountId: string;
  fromIndex: number;
  toIndex: number;
  x: number;
  step: number;
  settling: boolean;
};

type PendingViewportTransition = {
  promise: Promise<void>;
  resolve: () => void;
  timer: number;
};

const phaseOf = (a: AccountInfo): Phase =>
  a.in_game ? "in_game" : a.online ? "lobby" : "connecting";

const LIGHT: Record<Phase, string> = {
  in_game: "bg-red-500 shadow-[0_0_6px_1px_rgb(239_68_68/0.6)]",
  lobby: "bg-green-500 shadow-[0_0_6px_1px_rgb(34_197_94/0.6)]",
  connecting: "bg-yellow-400 shadow-[0_0_6px_1px_rgb(250_204_21/0.6)]",
};
const TEXT: Record<Phase, string> = {
  in_game: "text-red-400",
  lobby: "text-green-400",
  connecting: "text-yellow-300",
};

export function AccountTabs() {
  const { t } = useTranslation();
  const { accounts, refresh } = useAccounts();
  const [first, setFirst] = useState(injectedFirst);
  const firstRef = useRef(first);
  const [viewportMotion, setViewportMotion] = useState(false);
  const viewportTransition = useRef<PendingViewportTransition | null>(null);
  const [displayIds, setDisplayIds] = useState<string[] | null>(null);
  const [dragVisual, setDragVisual] = useState<DragVisual | null>(null);
  const [skipMotion, setSkipMotion] = useState(false);
  const previewRef = useRef<string[] | null>(null);
  const pointer = useRef<{
    id: number;
    accountId: string;
    startX: number;
    startY: number;
    lastX: number;
    lastY: number;
    dragging: boolean;
    cancelled: boolean;
    timer: number;
    node: HTMLDivElement;
    fromIndex?: number;
    toIndex?: number;
    grabOffset?: number;
    width?: number;
    step?: number;
    baseIds?: string[];
    openIds?: string[];
  } | null>(null);
  const rowNode = useRef<HTMLDivElement | null>(null);
  const arrowNodes = useRef(new Map<-1 | 1, HTMLButtonElement>());
  const lastArrowMove = useRef(0);
  const suppressPick = useRef(false);
  const [checkingLogout, setCheckingLogout] = useState<string | null>(null);
  const [askingLogout, setAskingLogout] = useState<{
    account: AccountInfo;
    inGame: boolean;
  } | null>(null);
  const [loggingOut, setLoggingOut] = useState(false);

  useEffect(
    () => () => {
      if (pointer.current) window.clearTimeout(pointer.current.timer);
      const pending = viewportTransition.current;
      if (pending) {
        window.clearTimeout(pending.timer);
        pending.resolve();
        viewportTransition.current = null;
      }
    },
    [],
  );

  const finishViewportTransition = () => {
    const pending = viewportTransition.current;
    if (!pending) return;
    window.clearTimeout(pending.timer);
    viewportTransition.current = null;
    setViewportMotion(false);
    pending.resolve();
  };

  const beginViewportTransition = () => {
    const previous = viewportTransition.current;
    if (previous) {
      window.clearTimeout(previous.timer);
      previous.resolve();
    }
    let resolve = () => {};
    const promise = new Promise<void>((done) => {
      resolve = done;
    });
    const pending: PendingViewportTransition = {
      promise,
      resolve,
      timer: 0,
    };
    // `transitionend` is authoritative. The timeout only prevents a blocked
    // account switch if WebView2 drops the event while the window is closing.
    pending.timer = window.setTimeout(
      () => finishViewportTransition(),
      VIEWPORT_TRANSITION_MS + 100,
    );
    viewportTransition.current = pending;
    setViewportMotion(true);
  };

  useLayoutEffect(() => {
    const syncBeforeShow = (event: Event) => {
      const next = (event as CustomEvent<number>).detail;
      if (!Number.isInteger(next) || next < 0) return;
      flushSync(() => {
        setViewportMotion(false);
        if (next === firstRef.current) return;
        firstRef.current = next;
        setFirst(next);
      });
    };
    window.addEventListener(VIEWPORT_SYNC_EVENT, syncBeforeShow);
    return () =>
      window.removeEventListener(VIEWPORT_SYNC_EVENT, syncBeforeShow);
  }, []);

  useEffect(() => {
    let live = true;
    let stop: (() => void) | undefined;
    const accept = (next: number) => {
      if (
        !live ||
        !Number.isInteger(next) ||
        next < 0 ||
        next === firstRef.current
      )
        return;
      setViewportMotion(false);
      firstRef.current = next;
      setFirst(next);
    };
    void listen<number>("account-tabs-first-changed", accept).then(
      (unlisten) => {
        if (!live) return unlisten();
        stop = unlisten;
        void invoke<number>("get_account_tabs_first")
          .then(accept)
          .catch(() => {});
      },
    );
    return () => {
      live = false;
      stop?.();
    };
  }, []);

  const orderedAccounts = useMemo(() => {
    if (!displayIds) return accounts;
    const byId = new Map(accounts.map((account) => [account.id, account]));
    const ordered = displayIds.flatMap((id) => {
      const account = byId.get(id);
      if (!account) return [];
      byId.delete(id);
      return [account];
    });
    return [...ordered, ...byId.values()];
  }, [accounts, displayIds]);

  const open = orderedAccounts.filter((a) => a.running);
  if (open.length === 0) return null;

  const last = Math.max(0, open.length - VISIBLE);
  const start = Math.min(first, last);
  const shown = Math.min(open.length, VISIBLE);

  const name = (a: AccountInfo) => accountLabel(a, t);

  const showFrom = (next: number) => {
    const clamped = Math.min(last, Math.max(0, next));
    if (clamped === firstRef.current) return;
    beginViewportTransition();
    firstRef.current = clamped;
    setFirst(clamped);
    void invoke("set_account_tabs_first", { first: clamped }).catch((error) => {
      toast.error(String(error));
    });
  };

  const pick = async (a: AccountInfo) => {
    if (a.current || suppressPick.current) return;
    try {
      // Every hidden account view already holds the final shared position.
      // Do not swap WebViews while this visible row is between positions:
      // doing so changes from an in-flight pixel to the final pixel and looks
      // like a rebound even though both views have the same logical index.
      await viewportTransition.current?.promise;
      await invoke("switch_account", { id: a.id });
    } catch (e) {
      toast.error(String(e));
    }
  };

  const saveOrder = async (ids: string[]) => {
    const animation = new Promise((resolve) => window.setTimeout(resolve, 200));
    try {
      await invoke("set_account_order", { ids });
      await refresh();
    } catch (error) {
      toast.error(t("play.tabs.order_failed"), { description: String(error) });
    } finally {
      await animation;
      // The cards are already visually in their final slots. Disable the
      // transform transition for the one render that hands those positions
      // over to the reordered DOM, or the dragged card animates a second time.
      setSkipMotion(true);
      previewRef.current = null;
      setDragVisual(null);
      setDisplayIds(null);
      window.requestAnimationFrame(() => {
        window.requestAnimationFrame(() => setSkipMotion(false));
      });
    }
  };

  const startDrag = (pointerId: number) => {
    const active = pointer.current;
    if (
      !active ||
      active.id !== pointerId ||
      active.cancelled ||
      active.dragging
    )
      return;
    const row = rowNode.current;
    const fromIndex = open.findIndex(
      (account) => account.id === active.accountId,
    );
    if (!row || fromIndex < 0) return;
    const chip = active.node.getBoundingClientRect();
    const gap = Number.parseFloat(getComputedStyle(row).gap) || GAP_REM * 16;
    const ids = accounts.map((account) => account.id);
    active.dragging = true;
    active.fromIndex = fromIndex;
    active.toIndex = fromIndex;
    active.grabOffset = active.startX - chip.left;
    active.width = chip.width;
    active.step = chip.width + gap;
    active.baseIds = ids;
    active.openIds = open.map((account) => account.id);
    active.node.setPointerCapture?.(pointerId);
    previewRef.current = ids;
    setDisplayIds(ids);
    setDragVisual({
      accountId: active.accountId,
      fromIndex,
      toIndex: fromIndex,
      x: active.lastX - active.startX,
      step: active.step,
      settling: false,
    });
    suppressPick.current = true;
  };

  const beginPointer = (
    a: AccountInfo,
    e: React.PointerEvent<HTMLDivElement>,
  ) => {
    if (e.button !== 0 || (e.target as HTMLElement).closest("[data-no-drag]"))
      return;
    const active = {
      id: e.pointerId,
      accountId: a.id,
      startX: e.clientX,
      startY: e.clientY,
      lastX: e.clientX,
      lastY: e.clientY,
      dragging: false,
      cancelled: false,
      timer: 0,
      node: e.currentTarget,
    };
    active.timer = window.setTimeout(
      () => startDrag(e.pointerId),
      LONG_PRESS_MS,
    );
    pointer.current = active;
    lastArrowMove.current = e.timeStamp - 181;
  };

  const movePointer = (e: React.PointerEvent<HTMLDivElement>) => {
    const active = pointer.current;
    if (!active || active.id !== e.pointerId) return;
    active.lastX = e.clientX;
    active.lastY = e.clientY;
    if (!active.dragging) {
      if (
        Math.hypot(e.clientX - active.startX, e.clientY - active.startY) <=
        MOVE_TOLERANCE_PX
      )
        return;
      active.cancelled = true;
      window.clearTimeout(active.timer);
      suppressPick.current = true;
      return;
    }
    e.preventDefault();

    const row = rowNode.current;
    if (
      !row ||
      active.fromIndex == null ||
      active.grabOffset == null ||
      active.width == null ||
      active.step == null ||
      !active.baseIds ||
      !active.openIds
    )
      return;
    const rowLeft = row.getBoundingClientRect().left;
    const desiredLeft = e.clientX - active.grabOffset;
    const x = desiredLeft - (rowLeft + active.fromIndex * active.step);
    const center = desiredLeft + active.width / 2;
    const toIndex = Math.min(
      active.openIds.length - 1,
      Math.max(
        0,
        Math.round((center - rowLeft - active.width / 2) / active.step),
      ),
    );
    if (toIndex !== active.toIndex) {
      active.toIndex = toIndex;
      previewRef.current = swapAccountIds(
        active.baseIds,
        active.accountId,
        active.openIds[toIndex],
      );
    }
    setDragVisual({
      accountId: active.accountId,
      fromIndex: active.fromIndex,
      toIndex,
      x,
      step: active.step,
      settling: false,
    });

    const arrowDir = [...arrowNodes.current].find(([, node]) => {
      const rect = node.getBoundingClientRect();
      return (
        e.clientX >= rect.left &&
        e.clientX <= rect.right &&
        e.clientY >= rect.top &&
        e.clientY <= rect.bottom
      );
    })?.[0];
    const now = e.timeStamp;
    if (
      (arrowDir === -1 || arrowDir === 1) &&
      now - lastArrowMove.current > 180
    ) {
      lastArrowMove.current = now;
      showFrom(firstRef.current + arrowDir);
    }
  };

  const endPointer = (e: React.PointerEvent<HTMLDivElement>, save: boolean) => {
    const active = pointer.current;
    if (!active || active.id !== e.pointerId) return;
    window.clearTimeout(active.timer);
    pointer.current = null;
    if (e.currentTarget.hasPointerCapture?.(e.pointerId)) {
      e.currentTarget.releasePointerCapture(e.pointerId);
    }
    if (!active.dragging) {
      if (active.cancelled) {
        window.setTimeout(() => {
          suppressPick.current = false;
        }, 0);
      }
      return;
    }
    e.preventDefault();
    const ids = previewRef.current;
    if (save && ids) {
      setDragVisual((current) =>
        current &&
        active.fromIndex != null &&
        active.toIndex != null &&
        active.step != null
          ? {
              ...current,
              x: (active.toIndex - active.fromIndex) * active.step,
              settling: true,
            }
          : current,
      );
      void saveOrder(ids);
    } else {
      previewRef.current = null;
      setDragVisual(null);
      setDisplayIds(null);
    }
    window.setTimeout(() => {
      suppressPick.current = false;
    }, 0);
  };

  const askLogout = async (a: AccountInfo) => {
    if (open.length === 1 || checkingLogout || loggingOut) return;
    setCheckingLogout(a.id);
    try {
      const inGame = await invoke<boolean>("account_in_game", { id: a.id });
      setAskingLogout({ account: a, inGame });
    } catch (error) {
      toast.error(t("play.tabs.logout_failed"), { description: String(error) });
    } finally {
      setCheckingLogout(null);
    }
  };

  const logout = async () => {
    if (!askingLogout || loggingOut) return;
    setLoggingOut(true);
    try {
      await invoke("logout_account", {
        id: askingLogout.account.id,
        allowInGame: askingLogout.inGame,
      });
      setAskingLogout(null);
    } catch (error) {
      if (String(error).includes("account_in_game")) {
        setAskingLogout((current) =>
          current ? { ...current, inGame: true } : current,
        );
      } else {
        toast.error(t("play.tabs.logout_failed"), {
          description: String(error),
        });
      }
    } finally {
      setLoggingOut(false);
      refresh();
    }
  };

  const arrow = (dir: -1 | 1, enabled: boolean) => (
    <button
      type="button"
      disabled={!enabled}
      onClick={() => showFrom(start + dir)}
      ref={(node) => {
        if (node) arrowNodes.current.set(dir, node);
        else arrowNodes.current.delete(dir);
      }}
      data-account-arrow={dir}
      className={cn(
        "flex h-7 w-6 shrink-0 items-center justify-center rounded-md text-muted-foreground transition-all duration-200",
        enabled
          ? "hover:bg-muted hover:text-foreground active:scale-90"
          : "opacity-0",
      )}
      aria-label={dir === 1 ? ">" : "<"}
    >
      {dir === 1 ? (
        <ChevronRight className="h-4 w-4" />
      ) : (
        <ChevronLeft className="h-4 w-4" />
      )}
    </button>
  );

  return (
    <>
      <div className="flex min-w-0 items-center gap-1">
        {open.length > VISIBLE && arrow(-1, start > 0)}
        <div
          className="overflow-hidden"
          style={{ width: `${shown * CHIP_REM + (shown - 1) * GAP_REM}rem` }}
        >
          <div
            ref={rowNode}
            className={cn(
              "flex",
              viewportMotion && "transition-transform duration-300 ease-out",
            )}
            onTransitionEnd={(event) => {
              if (event.propertyName === "transform")
                finishViewportTransition();
            }}
            style={{
              gap: `${GAP_REM}rem`,
              transform: `translateX(-${start * (CHIP_REM + GAP_REM)}rem)`,
            }}
          >
            {open.map((a, index) => {
              const phase = phaseOf(a);
              const isDragged = dragVisual?.accountId === a.id;
              let translate = 0;
              if (dragVisual) {
                if (isDragged) translate = dragVisual.x;
                else if (
                  dragVisual.toIndex > dragVisual.fromIndex &&
                  index > dragVisual.fromIndex &&
                  index <= dragVisual.toIndex
                ) {
                  translate = -dragVisual.step;
                } else if (
                  dragVisual.toIndex < dragVisual.fromIndex &&
                  index >= dragVisual.toIndex &&
                  index < dragVisual.fromIndex
                ) {
                  translate = dragVisual.step;
                }
              }
              return (
                <div
                  key={a.id}
                  data-account-id={a.id}
                  onPointerDown={(e) => beginPointer(a, e)}
                  onPointerMove={movePointer}
                  onPointerUp={(e) => endPointer(e, true)}
                  onPointerCancel={(e) => endPointer(e, false)}
                  style={{
                    width: `${CHIP_REM}rem`,
                    transform: `translate3d(${translate}px, 0, 0)`,
                    transition: skipMotion
                      ? "none"
                      : isDragged && !dragVisual?.settling
                        ? "box-shadow 160ms ease, border-color 160ms ease"
                        : "transform 180ms cubic-bezier(0.2, 0.8, 0.2, 1)",
                    willChange: dragVisual ? "transform" : undefined,
                    zIndex: isDragged ? 10 : undefined,
                  }}
                  className={cn(
                    "flex h-8 shrink-0 items-center rounded-lg border text-left text-xs",
                    "touch-none select-none cursor-grab overflow-hidden active:cursor-grabbing",
                    a.current
                      ? "border-primary/70 bg-primary/10"
                      : "border-border bg-muted/30 hover:border-foreground/30 hover:bg-muted/60",
                    isDragged && "shadow-lg ring-1 ring-primary/60",
                  )}
                >
                  <button
                    type="button"
                    onClick={() => void pick(a)}
                    title={name(a)}
                    className="flex h-full min-w-0 flex-1 items-center gap-2 px-2.5 text-left"
                  >
                    <span
                      className={cn(
                        "h-2 w-2 shrink-0 rounded-full transition-colors duration-300",
                        LIGHT[phase],
                      )}
                    />
                    <CharacterIcon id={a.character} className="h-5 w-5" />
                    <span className="flex min-w-0 flex-1 items-center gap-1">
                      <span className="min-w-0 flex-1 truncate font-medium">
                        {name(a)}
                      </span>
                      <span
                        className={cn(
                          "shrink-0 transition-colors duration-300",
                          TEXT[phase],
                        )}
                      >
                        {t(`play.tabs.${phase}`)}
                      </span>
                    </span>
                  </button>
                  <button
                    type="button"
                    data-no-drag
                    disabled={
                      open.length === 1 || checkingLogout !== null || loggingOut
                    }
                    onClick={() => void askLogout(a)}
                    title={
                      open.length === 1
                        ? t("play.tabs.last_open")
                        : t("play.tabs.logout_title", { name: name(a) })
                    }
                    aria-label={t("play.tabs.logout_title", { name: name(a) })}
                    className="flex h-full w-7 shrink-0 items-center justify-center border-l border-border/70 text-muted-foreground transition-colors hover:bg-red-500/15 hover:text-red-400 disabled:cursor-not-allowed disabled:opacity-35"
                  >
                    {checkingLogout === a.id ? (
                      <Loader2 className="h-3.5 w-3.5 animate-spin" />
                    ) : (
                      <LogOut className="h-3.5 w-3.5" />
                    )}
                  </button>
                </div>
              );
            })}
          </div>
        </div>
        {open.length > VISIBLE && arrow(1, start < last)}
      </div>
      <Dialog
        open={askingLogout !== null}
        onOpenChange={(open) => !open && !loggingOut && setAskingLogout(null)}
      >
        <DialogContent>
          <DialogHeader>
            <DialogTitle>
              {askingLogout?.inGame
                ? t("play.tabs.logout_in_game", {
                    name: askingLogout ? name(askingLogout.account) : "",
                  })
                : t("play.tabs.logout_body", {
                    name: askingLogout ? name(askingLogout.account) : "",
                  })}
            </DialogTitle>
          </DialogHeader>
          <DialogFooter className="mx-0 mb-0 border-0 bg-transparent p-0">
            <Button
              variant="outline"
              size="sm"
              disabled={loggingOut}
              onClick={() => setAskingLogout(null)}
            >
              {t("common.cancel")}
            </Button>
            <Button
              variant="destructive"
              size="sm"
              disabled={loggingOut}
              onClick={() => void logout()}
            >
              {loggingOut && <Loader2 className="h-4 w-4 animate-spin" />}
              {t("play.tabs.logout_confirm")}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </>
  );
}
