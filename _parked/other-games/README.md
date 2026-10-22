# 暫存區：天鳳（Tenhou）與麻雀一番街（Riichi City）

2026-10-19 起 Mokora 只支援雀魂，這兩個遊戲的實作整個移到這裡，沒有刪除。
這個資料夾不在編譯範圍內：`cargo`、`vite`、`tsc` 都不會讀它。

原作者的架構有清楚的「遊戲插槽」（`Platform` 列舉、`Bridge` 特徵、`BridgeHooks`、
`PlatformAutoplay` 特徵、`Step`），這些**都還在**，只是目前只有雀魂一個實作。
所以之後加新遊戲不用改架構，照下面的「新增一個遊戲」做就好；這兩個遊戲
則是兩種不同類型的完整範例。

## 內容

```text
files/                          整個移出來的檔案，路徑跟原本一樣
  src/bridge/tenhou/            天鳳橋接：純 JSON 走 WebSocket，轉成 mjai（含 README）
  src/bridge/riichi_city/       一番街橋接：15 位元組二進位標頭包 JSON（含 README）
  src/autoplay/tenhou/          天鳳自動打牌：按用戶端自己的 DOM 按鈕、呼叫它的丟牌函式
                                （inject.rs 在傳輸途中改寫用戶端腳本，讓丟牌函式拿得到）
  src/autoplay/tenhou_state.rs  橋接寫、自動打牌讀：手牌（天鳳牌號）與目前的決策視窗
  src/autoplay/riichi_city/     一番街自動打牌：規劃出協定封包、局間自動按「確定」
  src/autoplay/inject.rs        封包注入通道（給沒有瀏覽器頁面的原生用戶端）
  tests/tenhou_reconnect.rs     天鳳斷線重連的整合測試
  frontend/src/i18n/resources/<語言>.removed.json   被拿掉的翻譯
patches/shared-files.patch      其餘共用檔案裡被剪掉的部分（停放前後的差異，當參照用）
patches/pt-tenhou.patch         2026-10-23 再停放的部分：歷史紀錄的「天鳳」PT 規則（段位表與計分）、
                                歷史篩選的天鳳／一番街選項；在專案根目錄 `git apply` 就放回去
```

## 兩個遊戲的做法（想參考時看這裡）

| | 天鳳 | 一番街 |
|---|---|---|
| 用戶端 | 網頁（HTML＋腳本） | 原生程式，沒有網頁 |
| 擷取 | Chromium（CDP）或 MITM 都行 | 只能 MITM 代理 |
| 自動打牌 | 按用戶端自己的 DOM 按鈕、呼叫它的丟牌函式（直接寫封包會讓棋盤卡住） | 規劃出協定封包，由代理在「用戶端→伺服器」那條連線上注入 |
| 要用到的 Step | `DomClick`、`AwaitReady`、`Discard` | `SendFrame` |
| 額外的橋接資料 | `BridgeHooks.tenhou_state` | `BridgeHooks.riichi_inject` |

## 當時接在哪裡（剪掉的共用部分，細節看 patch）

- `config/platform.rs`：`Platform` 的兩個變體與 `subdir`、`From<Platform>` 的分支。
  現在的 `Platform` 只有 `Majsoul`，並用 `#[serde(other)]`，舊設定檔寫著天鳳或一番街會讀成雀魂。
- `bridge/mod.rs`：兩個 `mod`／`pub use`、`for_platform` 的兩個分支、`BridgeHooks` 的 `tenhou_state` 與 `riichi_inject`。
- `autoplay/platform.rs`：`Step` 的 `DomClick`／`AwaitReady`／`Discard`／`SendFrame`，`ActionContext.tenhou`。
- `autoplay/context.rs`：`tenhou_state`、`inject` 兩個共享欄位。
- `autoplay/manager.rs`：兩個遊戲的欄位與選擇分支、一番街的局間「確定」監看任務與逐決策執行任務、
  天鳳的決策視窗過期檢查（`planned_window`／`acted_window`／`tenhou_window_moved`）、
  `await_turn_ready`／`press_when_offered`、各 `Step` 的執行分支。
- `autoplay/cdp_input.rs`：「Tenhou actuation」整段（動作選單常數、`click_dom`、`turn_clock_running`、`discard_tile`）。
- `capture/chromium/cdp.rs`：攔截並改寫天鳳用戶端腳本（`rewrite_paused_script`）、載入後重載（`reload_if_uninstrumented`）、
  `enable_interception` 裡的回應階段規則，以及 `attach_page` 的 `notify` 參數；`capture/chromium/mod.rs` 的 hooks。
- `proxy/handler.rs`、`proxy/mod.rs`、`capture/hudsucker_backend.rs`：`start_proxy`／`ProxyHandler::new` 的 `inject` 參數、
  WebSocket 上的注入轉送、一番街的明文遙測路徑封鎖（`is_riichi_city_telemetry_path`）。
- 整合測試 `tests/*` 呼叫 `start_proxy` 時的 `inject` 引數。
- 前端：`lib/platforms.ts` 的兩筆、`types.ts` 的 `PlatformKind`、設定頁的一番街自動打牌警告，翻譯的 `platform.tenhou*`／`platform.riichi_city*`。

沒有動的：歷史紀錄的資料格式（`schema::Platform` 與 `MatchInfo` 仍認得天鳳與一番街，舊紀錄照常讀得出來）、
訓練工具裡用天鳳牌譜的部分（`native_bot/src/bin/*`）、規則名稱 `default_tenhou`（riichienv 的預設規則）、
`announcements`（原作者的更新紀錄）。PT 規則的「天鳳段位」原本留著，2026-10-23 也停放了
（`patches/pt-tenhou.patch`；存過天鳳規則的使用者會回到預設的雀魂規則），自動打牌說明裡提到天鳳的句子
一併改掉（原文在 `<語言>.removed.json`）。

## 新增一個遊戲（照這個順序）

1. `config::Platform` 加變體；`subdir`、`From<Platform>` 補分支；前端 `lib/platforms.ts` 加一筆（網址、是否支援 Chromium）。
2. 選擷取方式：網頁用戶端走 Chromium（`capture/chromium/`，到 `AUTOPLAY_HOST_HINTS` 加網域），原生用戶端走 MITM（`proxy/`）。
3. 寫橋接：`src/bridge/<遊戲>/mod.rs` 實作 `Bridge`（封包 → mjai 事件），在 `bridge::for_platform` 加分支；
   需要跟自動打牌共用的資料放進 `BridgeHooks`。
4. 歷史紀錄：`schema::Platform`／`MatchInfo` 補變體與房間標籤（`frontend/src/lib/matchInfo.ts`）。
5. 自動打牌（選用）：實作 `PlatformAutoplay`；用戶端不是畫布就替 `Step` 加新種類並在 `manager.rs` 寫執行分支，
   在 `handle_bot_response` 加分支。
6. 測試用合成的封包，不要貼真實擷取（裡面有帳號 id）。
7. 翻譯四個語言檔。

## 要把天鳳或一番街放回去時

1. 把 `files/` 底下的檔案複製回專案同樣的路徑。
2. 把 `patches/shared-files.patch` 反向套用（`git apply -R`，或照 patch 內容手動改）；
   中間若又改過那些檔案就會有衝突，需要手動處理。
3. 把 `*.removed.json` 的翻譯合回語言檔。
