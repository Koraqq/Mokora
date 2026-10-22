//! Full auto's map of the Mahjong Soul lobby: where to press, and which part
//! of the screen must show what, for the next-game flow
//! (`autoplay::next_game` in Mokora) to start the next ranked game by
//! itself. The module holds the values ([`crate::method::LOBBY_LAYOUT`]);
//! Mokora runs the flow with them.
//!
//! Points are `(x, y)` on the client's 16x9 grid; boxes `(x_min, x_max,
//! y_min, y_max)` on it; shares are of a box's pixels.

use serde::{Deserialize, Serialize};

pub type Point = (f64, f64);
pub type Box4 = (f64, f64, f64, f64);

/// The lobby and the end-of-game screens.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Layout {
    /// The confirm button of every end-of-game screen.
    pub result_confirm: Point,
    /// A tap closing the room chest's reward screen.
    pub chest_dismiss: Point,
    /// The profile icon on the lobby's name plate, its window's ✕ and its
    /// 3-player switch.
    pub lobby_avatar: Point,
    pub profile_close: Point,
    pub profile_3p_switch: Point,
    /// 活動, its 復活幣 tab, 領取, a tap closing the reward banner, and the
    /// page's return arrow.
    pub lobby_activity: Point,
    pub activity_revive_tab: Point,
    pub revive_claim: Point,
    pub reward_dismiss: Point,
    pub activity_close: Point,
    /// The "銅幣不足" dialog's 確認, its yellow face and the share of it.
    pub coins_dialog_confirm: Point,
    pub coins_dialog_box: Box4,
    pub coins_dialog_min_yellow: f64,
    /// The bond story's 跳過, its outline and the share of its cyan.
    pub bond_skip: Point,
    pub bond_skip_box: Box4,
    pub bond_skip_min_cyan: f64,
    /// The promotion certificate's 關閉 X, and how it is told: parchment and
    /// the X's red.
    pub rank_cert_close: Point,
    pub rank_cert_parchment_box: Box4,
    pub rank_cert_close_box: Box4,
    pub rank_cert_min_parchment: f64,
    pub rank_cert_min_red: f64,
    /// The confirm button's yellow face, and the share of it.
    pub result_button_box: Box4,
    pub result_button_min_yellow: f64,
    /// The strip down the lobby's signs, its navy and warm rows (fractions of
    /// its height) and the shares of each.
    pub lobby_strip: Box4,
    pub lobby_navy_rows: (f64, f64),
    pub lobby_warm_rows: (f64, f64),
    pub lobby_navy_min: f64,
    pub lobby_warm_min: f64,
    /// 段位場 on the lobby.
    pub lobby_ranked: Point,
    /// "<返回" over the ranked lists, its label and the share of its gold.
    pub list_back: Point,
    pub list_back_box: Box4,
    pub list_back_min_tan: f64,
    /// Logging out and back in: the gear, 登出帳號, its 確認 (and that
    /// button's yellow face), 進入遊戲 (and its face), the share of yellow
    /// for both, "切換 >>", and the route list's first row, step and rows.
    pub settings_gear: Point,
    pub settings_log_out: Point,
    pub log_out_confirm: Point,
    pub log_out_confirm_box: Box4,
    pub login_enter: Point,
    pub login_enter_box: Box4,
    pub login_min_yellow: f64,
    pub login_switch_route: Point,
    pub login_route_first: Point,
    pub login_route_step: f64,
    pub login_route_rows: usize,
    /// Where the room and table lists are scrolled from.
    pub list_anchor: Point,
    /// The room list at its top (銅 銀 金) and its bottom (金 玉 王座).
    pub room_top: [Point; 3],
    pub room_bottom: [Point; 3],
    /// A room's tables at the top of its list (四人東 四人南 三人東), and the
    /// last (三人南) at its bottom.
    pub table_top: [Point; 3],
    pub table_bottom_last: Point,
}
