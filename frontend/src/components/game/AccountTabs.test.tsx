import { beforeEach, describe, expect, it, vi } from "vitest";
import { act, fireEvent, render, screen } from "@testing-library/react";

vi.mock("react-i18next", () => ({
  useTranslation: () => ({
    t: (key: string) => key,
    i18n: { language: "en", changeLanguage: vi.fn() },
  }),
  initReactI18next: { type: "3rdParty", init: () => {} },
}));

const invoke = vi.fn();
const listen = vi.fn((name: string, callback: (payload: number) => void) => {
  void name;
  void callback;
  return Promise.resolve(() => {});
});
vi.mock("@/lib/tauri", () => ({
  invoke: (...args: unknown[]) => invoke(...args),
  listen: (name: string, callback: (payload: number) => void) =>
    listen(name, callback),
  HAS_TAURI: true,
}));

import type { AccountInfo } from "@/lib/accounts";

function account(
  id: string,
  nickname: string,
  current = false,
  inGame = false,
): AccountInfo {
  return {
    id,
    config_path: `${id}/config.toml`,
    identity: { account_id: Number(id) || 1, nickname },
    running: true,
    online: true,
    browser: true,
    current,
    startup: false,
    duplicate: false,
    last_login: null,
    avatar: null,
    in_game: inGame,
    character: null,
    locked: false,
  };
}

let accounts: AccountInfo[] = [];
const refresh = vi.fn();
vi.mock("@/hooks/useAccounts", () => ({
  useAccounts: () => ({ accounts, refresh }),
}));

import { AccountTabs } from "./AccountTabs";

const logoutButtons = () => screen.getAllByLabelText("play.tabs.logout_title");

describe("AccountTabs", () => {
  beforeEach(() => {
    accounts = [account("main", "Mine", true), account("2", "Other")];
    invoke.mockReset();
    listen.mockClear();
    refresh.mockReset();
    invoke.mockImplementation((command: string) => {
      if (command === "account_in_game") return Promise.resolve(false);
      return Promise.resolve(undefined);
    });
    delete (
      window as typeof window & { __MOKORA_ACCOUNT_TABS_FIRST__?: number }
    ).__MOKORA_ACCOUNT_TABS_FIRST__;
  });

  it("keeps a fixed logout slot while only the long name is truncated", () => {
    accounts[0].identity = {
      account_id: 1,
      nickname: "A nickname far longer than the account chip can display",
    };
    render(<AccountTabs />);

    const name = screen.getByText(accounts[0].identity.nickname);
    expect(name.className).toContain("truncate");
    expect(name.className).toContain("flex-1");
    expect(logoutButtons()[0].className).toContain("w-7");
    expect(screen.getAllByText("play.tabs.lobby")[0].className).toContain(
      "shrink-0",
    );
  });

  it("saves the complete account order after a drag", async () => {
    vi.useFakeTimers();
    accounts = [
      account("main", "Mine", true),
      account("2", "Second"),
      account("3", "Third"),
    ];
    render(<AccountTabs />);
    const mine = screen.getByTitle("Mine").parentElement!;
    const row = mine.parentElement!;
    vi.spyOn(mine, "getBoundingClientRect").mockReturnValue({
      left: 0,
      right: 192,
      top: 0,
      bottom: 32,
      width: 192,
      height: 32,
      x: 0,
      y: 0,
      toJSON: () => ({}),
    });
    vi.spyOn(row, "getBoundingClientRect").mockReturnValue({
      left: 0,
      right: 592,
      top: 0,
      bottom: 32,
      width: 592,
      height: 32,
      x: 0,
      y: 0,
      toJSON: () => ({}),
    });

    fireEvent.pointerDown(mine, {
      button: 0,
      pointerId: 7,
      clientX: 10,
      clientY: 10,
    });
    act(() => vi.advanceTimersByTime(250));
    fireEvent.pointerMove(mine, { pointerId: 7, clientX: 410, clientY: 10 });
    expect(mine.style.transform).toContain("translate3d(400px");
    expect(mine.style.transform).not.toContain("scale");
    fireEvent.pointerUp(mine, { pointerId: 7, clientX: 410, clientY: 10 });

    expect(invoke).toHaveBeenCalledWith("set_account_order", {
      ids: ["2", "3", "main"],
    });
    vi.useRealTimers();
  });

  it("switches accounts on a short click without starting a drag", async () => {
    render(<AccountTabs />);
    const other = screen.getByTitle("Other");
    const chip = other.parentElement!;
    const order = () =>
      [...document.querySelectorAll("[data-account-id]")].map((node) =>
        node.getAttribute("data-account-id"),
      );
    const before = order();

    fireEvent.pointerDown(chip, {
      button: 0,
      pointerId: 8,
      clientX: 210,
      clientY: 10,
    });
    fireEvent.pointerUp(chip, { pointerId: 8, clientX: 210, clientY: 10 });
    await act(async () => fireEvent.click(other));

    expect(invoke).toHaveBeenCalledWith("switch_account", { id: "2" });
    expect(invoke).not.toHaveBeenCalledWith(
      "set_account_order",
      expect.anything(),
    );
    expect(refresh).not.toHaveBeenCalled();
    expect(order()).toEqual(before);
    expect(other.className).not.toContain("scale");
  });

  it("does not begin reordering until the 250 ms long press completes", () => {
    vi.useFakeTimers();
    render(<AccountTabs />);
    const mine = screen.getByTitle("Mine").parentElement!;

    fireEvent.pointerDown(mine, {
      button: 0,
      pointerId: 9,
      clientX: 10,
      clientY: 10,
    });
    act(() => vi.advanceTimersByTime(249));
    expect(mine.style.zIndex).toBe("");
    act(() => vi.advanceTimersByTime(1));
    expect(mine.style.zIndex).toBe("10");
    fireEvent.pointerCancel(mine, { pointerId: 9 });
    vi.useRealTimers();
  });

  it("keeps the manually scrolled three-account window when the fourth account is clicked", async () => {
    accounts = [
      account("main", "Mine", true),
      account("2", "Second"),
      account("3", "Third"),
      account("4", "Fourth"),
    ];
    (
      window as typeof window & { __MOKORA_ACCOUNT_TABS_FIRST__?: number }
    ).__MOKORA_ACCOUNT_TABS_FIRST__ = 1;
    invoke.mockImplementation((command: string) => {
      if (command === "get_account_tabs_first") return Promise.resolve(1);
      return Promise.resolve(undefined);
    });
    const { rerender } = render(<AccountTabs />);
    const row = document.querySelector(
      '[data-account-id="main"]',
    )!.parentElement!;
    const before = row.style.transform;

    await act(async () => fireEvent.click(screen.getByTitle("Fourth")));

    expect(invoke).toHaveBeenCalledWith("switch_account", { id: "4" });
    expect(invoke).not.toHaveBeenCalledWith(
      "set_account_tabs_first",
      expect.anything(),
    );
    expect(row.style.transform).toBe(before);
    expect(row.style.transform).toBe("translateX(-12.5rem)");

    accounts = accounts.map((item) => ({
      ...item,
      current: item.id === "4",
    }));
    rerender(<AccountTabs />);

    expect(row.style.transform).toBe("translateX(-12.5rem)");
  });

  it("shares the account window only when an arrow is clicked", async () => {
    accounts = [
      account("main", "Mine", true),
      account("2", "Second"),
      account("3", "Third"),
      account("4", "Fourth"),
    ];
    render(<AccountTabs />);
    const row = document.querySelector(
      '[data-account-id="main"]',
    )!.parentElement!;

    await act(async () => fireEvent.click(screen.getByLabelText(">")));

    expect(invoke).toHaveBeenCalledWith("set_account_tabs_first", { first: 1 });
    expect(row.className).toContain("transition-transform");
  });

  it("finishes the shared viewport movement before switching WebViews", async () => {
    accounts = [
      account("main", "Mine", true),
      account("2", "Second"),
      account("3", "Third"),
      account("4", "Fourth"),
    ];
    render(<AccountTabs />);
    const row = document.querySelector(
      '[data-account-id="main"]',
    )!.parentElement!;

    fireEvent.click(screen.getByLabelText(">"));
    fireEvent.click(screen.getByTitle("Second"));

    expect(invoke).not.toHaveBeenCalledWith("switch_account", { id: "2" });

    await act(async () => {
      fireEvent.transitionEnd(row, { propertyName: "transform" });
    });

    expect(invoke).toHaveBeenCalledWith("switch_account", { id: "2" });
  });

  it("syncs a hidden account window before its switched view is painted", () => {
    accounts = [
      account("main", "Mine", true),
      account("2", "Second"),
      account("3", "Third"),
      account("4", "Fourth"),
    ];
    const { rerender } = render(<AccountTabs />);
    const row = document.querySelector(
      '[data-account-id="main"]',
    )!.parentElement!;
    expect(row.style.transform).toBe("translateX(-0rem)");

    act(() => {
      window.dispatchEvent(
        new CustomEvent("mokora-account-tabs-sync", { detail: 1 }),
      );
    });
    expect(row.style.transform).toBe("translateX(-12.5rem)");
    expect(row.className).not.toContain("transition-transform");

    accounts = accounts.map((item) => ({
      ...item,
      current: item.id === "2",
    }));
    rerender(<AccountTabs />);
    expect(row.style.transform).toBe("translateX(-12.5rem)");
  });

  it("always asks before closing an account that is in the lobby", async () => {
    render(<AccountTabs />);
    await act(async () => {
      fireEvent.click(logoutButtons()[1]);
    });
    expect(invoke).toHaveBeenCalledWith("account_in_game", { id: "2" });
    expect(screen.getByText("play.tabs.logout_body")).toBeTruthy();
    expect(screen.getByRole("heading").textContent).toBe(
      "play.tabs.logout_body",
    );
    expect(screen.queryByText("play.tabs.logout")).toBeNull();

    await act(async () => {
      fireEvent.click(screen.getByText("play.tabs.logout_confirm"));
    });
    expect(invoke).toHaveBeenCalledWith("logout_account", {
      id: "2",
      allowInGame: false,
    });
  });

  it("warns before closing an account that is already in a game", async () => {
    accounts[1].in_game = true;
    invoke.mockImplementation((command: string) => {
      if (command === "account_in_game") return Promise.resolve(true);
      return Promise.resolve(undefined);
    });
    render(<AccountTabs />);
    await act(async () => {
      fireEvent.click(logoutButtons()[1]);
    });
    expect(screen.getByText("play.tabs.logout_in_game")).toBeTruthy();

    await act(async () => {
      fireEvent.click(screen.getByText("play.tabs.logout_confirm"));
    });
    expect(invoke).toHaveBeenCalledWith("logout_account", {
      id: "2",
      allowInGame: true,
    });
  });

  it("asks again if the account enters a game after the dialog opened", async () => {
    let firstLogout = true;
    invoke.mockImplementation((command: string) => {
      if (command === "account_in_game") return Promise.resolve(false);
      if (command === "logout_account" && firstLogout) {
        firstLogout = false;
        return Promise.reject("account_in_game");
      }
      return Promise.resolve(undefined);
    });
    render(<AccountTabs />);
    await act(async () => {
      fireEvent.click(logoutButtons()[1]);
    });
    expect(screen.getByText("play.tabs.logout_body")).toBeTruthy();

    await act(async () => {
      fireEvent.click(screen.getByText("play.tabs.logout_confirm"));
    });
    expect(screen.getByText("play.tabs.logout_in_game")).toBeTruthy();

    await act(async () => {
      fireEvent.click(screen.getByText("play.tabs.logout_confirm"));
    });
    expect(invoke).toHaveBeenLastCalledWith("logout_account", {
      id: "2",
      allowInGame: true,
    });
  });

  it("keeps a visible but disabled logout slot for the final open account", () => {
    accounts = [account("main", "Mine", true)];
    render(<AccountTabs />);
    expect((logoutButtons()[0] as HTMLButtonElement).disabled).toBe(true);
    expect(logoutButtons()[0].getAttribute("title")).toBe(
      "play.tabs.last_open",
    );
  });
});
