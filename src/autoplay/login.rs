//! How each account logs in, learned from its own logins.
//!
//! Logging out and back in (the next-game flow's fresh session) is only
//! "press 進入遊戲" for an account the login screen keeps filled in, that is a
//! password or phone account. A Google or Facebook account is signed in from
//! the icon under the form instead, and that opens a browser window of the
//! provider's. Nothing here asks the player anything: every login is watched,
//! and what it showed is what the next one does.
//!
//! - A login by password (`.lq.Lobby.login`) is noted by the bridge.
//! - A Google or Facebook window opening is noted by the page poll
//!   ([`watch_popup`]), together with where on the game the player pressed to
//!   open it — the icon sits at the same place every time.
//! - On the way back in, the icon is pressed at that place and the provider's
//!   window is answered ([`watch_popup`] again, armed by the flow): the one
//!   account Google lists, or Facebook's "continue" button.
//!
//! The provider's pages are not Mokora's: they are read as little as
//! possible, and a page that is not as expected is left alone for the player.

use std::sync::Arc;
use std::time::{Duration, Instant};

use chromiumoxide::page::Page;
use serde::{Deserialize, Serialize};
use tracing::{info, warn};

use super::context::AutoplayContext;

/// How an account signs in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LoginMethod {
    /// E-mail or phone number with a password: the login screen keeps it
    /// filled in, so 進入遊戲 is all there is to press.
    Password,
    Google,
    Facebook,
}

/// What is known about how one account signs in.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LoginSetup {
    pub method: LoginMethod,
    /// Where the provider's icon was pressed, on the client's 16x9 grid.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub press: Option<(f64, f64)>,
    /// The Google account that was picked, as the chooser names it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub account: Option<String>,
}

impl LoginSetup {
    pub fn password() -> Self {
        Self {
            method: LoginMethod::Password,
            press: None,
            account: None,
        }
    }

    /// Where to press on the login screen for this account instead of
    /// 進入遊戲, if it signs in through a provider whose icon is known.
    pub fn provider_press(&self) -> Option<(f64, f64)> {
        match self.method {
            LoginMethod::Password => None,
            LoginMethod::Google | LoginMethod::Facebook => self.press,
        }
    }
}

/// Which provider an address belongs to.
pub fn method_of_url(url: &str) -> Option<LoginMethod> {
    let rest = url.split_once("://").map_or(url, |(_, rest)| rest);
    let host = rest.split(['/', '?', '#']).next()?;
    let host = host.rsplit('@').next()?.split(':').next()?;
    let host = host.to_ascii_lowercase();
    // Google's own sign-in host in any country domain (`.com`, `.com.tw`,
    // `.co.jp`): what follows is a short top-level name, not another site.
    let google_country = host.strip_prefix("accounts.google.").is_some_and(|rest| {
        let labels: Vec<&str> = rest.split('.').collect();
        labels.len() <= 2 && labels.iter().all(|l| !l.is_empty() && l.len() <= 3)
    });
    if google_country {
        Some(LoginMethod::Google)
    } else if host == "facebook.com" || host.ends_with(".facebook.com") {
        Some(LoginMethod::Facebook)
    } else {
        None
    }
}

/// An address that says nothing yet: a window that has just opened.
fn is_blank(url: &str) -> bool {
    url.is_empty() || url == "about:blank"
}

/// Records where the player presses on the game, on its 16x9 grid, so the
/// icon that opened a provider's window can be found again. Installed on the
/// game page for good (and on every page it loads next).
pub const RECORDER_JS: &str = "(()=>{if(window.__mokoraPress)return;\
    const log=[];window.__mokoraPress=log;\
    addEventListener('mousedown',e=>{\
      const c=document.getElementsByTagName('canvas')[0];if(!c)return;\
      const r=c.getBoundingClientRect();if(!r.width||!r.height)return;\
      log.push({gx:(e.clientX-r.x)/r.width*16,gy:(e.clientY-r.y)/r.height*9,t:Date.now()});\
      if(log.length>8)log.shift();\
    },{capture:true,passive:true});})()";

/// Reads the recorder: the time now, and the presses so far.
const READ_PRESSES_JS: &str = "(()=>({now:Date.now(),presses:window.__mokoraPress||[]}))()";

/// Remembers the account a Google chooser is answered with, in its window.
const PICK_RECORDER_JS: &str = "(()=>{if(window.__mokoraPick!==undefined)return;\
    window.__mokoraPick=null;\
    addEventListener('click',e=>{const el=e.target&&e.target.closest&&\
      e.target.closest('[data-identifier]');\
      if(el)window.__mokoraPick=el.getAttribute('data-identifier');},true);})()";

#[derive(Debug, Clone, Copy, PartialEq, Deserialize)]
struct Press {
    gx: f64,
    gy: f64,
    t: f64,
}

#[derive(Debug, Deserialize)]
struct Presses {
    now: f64,
    presses: Vec<Press>,
}

/// How long before a provider's window opened a press still counts as the
/// one that opened it.
const PRESS_WINDOW_MS: f64 = 6000.0;

/// The last press within the window before `now`, if it was on the canvas.
fn press_before(read: &Presses) -> Option<(f64, f64)> {
    read.presses
        .iter()
        .rev()
        .find(|p| read.now - p.t >= 0.0 && read.now - p.t <= PRESS_WINDOW_MS)
        .filter(|p| (0.0..=16.0).contains(&p.gx) && (0.0..=9.0).contains(&p.gy))
        .map(|p| (p.gx, p.gy))
}

/// Which of Google's listed accounts to take.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Pick {
    Index(usize),
    /// Nothing is listed yet.
    Nothing,
    /// Several are and none is the one remembered: for the player to choose.
    Ambiguous,
}

fn pick_account(listed: &[String], remembered: Option<&str>) -> Pick {
    if let Some(i) = remembered.and_then(|r| listed.iter().position(|id| id == r)) {
        return Pick::Index(i);
    }
    match listed.len() {
        0 => Pick::Nothing,
        1 => Pick::Index(0),
        _ => Pick::Ambiguous,
    }
}

/// Facebook's "continue as …" button among a dialog's buttons: the first
/// that has a label and is not the one that cancels.
fn pick_continue(labels: &[String]) -> Option<usize> {
    const CANCEL: [&str; 6] = ["cancel", "取消", "キャンセル", "not now", "以後再說", "稍后"];
    labels.iter().position(|label| {
        let lower = label.trim().to_lowercase();
        !lower.is_empty() && !CANCEL.iter().any(|word| lower.contains(word))
    })
}

const POPUP_ADDRESS_WAIT: Duration = Duration::from_secs(20);
const POPUP_ANSWER_WAIT: Duration = Duration::from_secs(25);
const POPUP_POLL: Duration = Duration::from_millis(400);

/// Look at a page that has just opened: if it is Google's or Facebook's
/// sign-in window, learn from it — or, while the next-game flow is logging an
/// account back in, answer it.
pub async fn watch_popup(page: Page, ctx: Arc<AutoplayContext>) {
    let started = Instant::now();
    let method = loop {
        let url = page.url().await.ok().flatten().unwrap_or_default();
        if let Some(method) = method_of_url(&url) {
            break method;
        }
        if !is_blank(&url) || started.elapsed() > POPUP_ADDRESS_WAIT {
            return;
        }
        tokio::time::sleep(POPUP_POLL).await;
    };

    if ctx.lobby.assist_armed().is_some() {
        info!("login: answering the {method:?} window");
        match answer(&page, method, &ctx).await {
            Ok(()) => ctx.lobby.disarm_assist(),
            Err(why) => warn!("login: the {method:?} window was left to the player: {why}"),
        }
        return;
    }

    // A person's login: where the icon was pressed, and how it went.
    let press = match ctx.page.read().await.clone() {
        Some(game) => match game.evaluate(READ_PRESSES_JS).await {
            Ok(value) => serde_json::from_value::<Presses>(value)
                .ok()
                .as_ref()
                .and_then(press_before),
            Err(_) => None,
        },
        None => None,
    };
    let previous = ctx.lobby.login_setup().filter(|s| s.method == method);
    let mut setup = LoginSetup {
        method,
        press: press.or(previous.as_ref().and_then(|s| s.press)),
        account: previous.and_then(|s| s.account),
    };
    info!("login: signed in through {method:?} (icon at {:?})", setup.press);
    ctx.lobby.note_login_setup(setup.clone());

    if method == LoginMethod::Google {
        // Which account the player picks, so a chooser listing several can
        // be answered the same way later.
        let _ = page.evaluate(PICK_RECORDER_JS).await;
        let mut picked = None;
        while page.url().await.is_ok() && started.elapsed() < Duration::from_secs(300) {
            if let Ok(result) = page.evaluate("window.__mokoraPick").await {
                if let Ok(Some(id)) = result.into_value::<Option<String>>() {
                    picked = Some(id);
                }
            }
            tokio::time::sleep(POPUP_POLL).await;
        }
        if let Some(account) = picked.filter(|a| setup.account.as_deref() != Some(a)) {
            setup.account = Some(account);
            ctx.lobby.note_login_setup(setup);
        }
    }
}

/// Press what the provider's window asks for.
async fn answer(page: &Page, method: LoginMethod, ctx: &AutoplayContext) -> Result<(), String> {
    let remembered = ctx.lobby.login_setup().and_then(|s| s.account);
    let started = Instant::now();
    loop {
        // The window going away on its own is the login having gone through.
        if page.url().await.is_err() {
            return Ok(());
        }
        match method {
            LoginMethod::Google => {
                let mut items = page
                    .find_elements("[data-identifier]")
                    .await
                    .unwrap_or_default();
                if items.is_empty() {
                    items = page.find_elements("[data-email]").await.unwrap_or_default();
                }
                let mut listed = Vec::new();
                for item in &items {
                    let id = match item.attribute("data-identifier").await {
                        Ok(Some(id)) => id,
                        _ => item
                            .attribute("data-email")
                            .await
                            .ok()
                            .flatten()
                            .unwrap_or_default(),
                    };
                    listed.push(id);
                }
                match pick_account(&listed, remembered.as_deref()) {
                    Pick::Index(i) => {
                        items[i].click().await.map_err(|e| format!("{e:#}"))?;
                        return Ok(());
                    }
                    Pick::Ambiguous => {
                        return Err(format!("{} accounts are listed", listed.len()));
                    }
                    Pick::Nothing => {}
                }
            }
            LoginMethod::Facebook => {
                let items = page
                    .find_elements("[name=\"__CONFIRM__\"], [role=\"button\"], button")
                    .await
                    .unwrap_or_default();
                let mut labels = Vec::new();
                for item in &items {
                    labels.push(item.inner_text().await.ok().flatten().unwrap_or_default());
                }
                if let Some(i) = pick_continue(&labels) {
                    items[i].click().await.map_err(|e| format!("{e:#}"))?;
                    return Ok(());
                }
            }
            LoginMethod::Password => return Ok(()),
        }
        if started.elapsed() > POPUP_ANSWER_WAIT {
            return Err("nothing to press came up".into());
        }
        tokio::time::sleep(POPUP_POLL).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_provider_is_told_from_the_address() {
        assert_eq!(
            method_of_url("https://accounts.google.com/o/oauth2/auth?client_id=1"),
            Some(LoginMethod::Google)
        );
        assert_eq!(
            method_of_url("https://accounts.google.com.tw/signin"),
            Some(LoginMethod::Google)
        );
        for url in [
            "https://www.facebook.com/v18.0/dialog/oauth?x=1",
            "https://m.facebook.com/login",
            "https://facebook.com/dialog",
        ] {
            assert_eq!(method_of_url(url), Some(LoginMethod::Facebook), "{url}");
        }
        for url in [
            "",
            "about:blank",
            "https://game.maj-soul.com/1/",
            "https://evil.com/?u=accounts.google.com",
            "https://accounts.google.evil.com/",
            "https://notfacebook.com/",
        ] {
            assert_eq!(method_of_url(url), None, "{url}");
        }
        assert!(is_blank("about:blank") && is_blank("") && !is_blank("https://a.b"));
    }

    #[test]
    fn the_press_that_opened_the_window_is_the_last_recent_one_on_the_canvas() {
        let press = |gx, gy, t| Press { gx, gy, t };
        let read = Presses {
            now: 10_000.0,
            presses: vec![press(3.0, 3.0, 1_000.0), press(6.5, 7.6, 8_000.0)],
        };
        assert_eq!(press_before(&read), Some((6.5, 7.6)));
        let stale = Presses {
            now: 10_000.0,
            presses: vec![press(6.5, 7.6, 1_000.0)],
        };
        assert_eq!(press_before(&stale), None);
        let off = Presses {
            now: 10_000.0,
            presses: vec![press(-1.0, 4.0, 9_000.0)],
        };
        assert_eq!(press_before(&off), None);
        assert_eq!(press_before(&Presses { now: 1.0, presses: vec![] }), None);
    }

    #[test]
    fn google_is_answered_with_the_one_account_or_the_remembered_one() {
        let ids = |list: &[&str]| list.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert_eq!(pick_account(&[], None), Pick::Nothing);
        assert_eq!(pick_account(&ids(&["a@x.com"]), None), Pick::Index(0));
        assert_eq!(pick_account(&ids(&["a@x.com", "b@x.com"]), None), Pick::Ambiguous);
        assert_eq!(
            pick_account(&ids(&["a@x.com", "b@x.com"]), Some("b@x.com")),
            Pick::Index(1)
        );
        // A remembered account that is no longer listed counts for nothing.
        assert_eq!(pick_account(&ids(&["a@x.com", "b@x.com"]), Some("c@x.com")), Pick::Ambiguous);
        assert_eq!(pick_account(&ids(&["a@x.com"]), Some("c@x.com")), Pick::Index(0));
    }

    #[test]
    fn facebook_continues_with_the_first_button_that_does_not_cancel() {
        let labels = |list: &[&str]| list.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert_eq!(pick_continue(&labels(&["以 Kora 的身分繼續", "取消"])), Some(0));
        assert_eq!(pick_continue(&labels(&["", "Cancel", "Continue as Kora"])), Some(2));
        assert_eq!(pick_continue(&labels(&["取消"])), None);
        assert_eq!(pick_continue(&[]), None);
    }

    #[test]
    fn only_a_provider_login_has_an_icon_to_press() {
        assert_eq!(LoginSetup::password().provider_press(), None);
        let google = LoginSetup {
            method: LoginMethod::Google,
            press: Some((6.0, 7.5)),
            account: None,
        };
        assert_eq!(google.provider_press(), Some((6.0, 7.5)));
        let unknown = LoginSetup { press: None, ..google.clone() };
        assert_eq!(unknown.provider_press(), None);
        let json = serde_json::to_string(&google).unwrap();
        assert_eq!(serde_json::from_str::<LoginSetup>(&json).unwrap(), google);
        assert!(!json.contains("account"));
    }
}
