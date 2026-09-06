//! Shared logical-pixel geometry for rendering, camera projection and hit testing.
use bevy::prelude::*;

use crate::constants::{BOARD_WORLD_SIZE, CENTER_DICE_HALO_OUTER_RADIUS};
use crate::platform::{DeviceProfile, HudLayoutMode};

pub(crate) const GLOBAL_SETTINGS_MARGIN: f32 = 16.0;
pub(crate) const GLOBAL_SETTINGS_SIZE: f32 = 48.0;
pub(crate) const GLOBAL_SETTINGS_RADIUS: f32 = 12.0;
pub(crate) const GLOBAL_SETTINGS_GAP: f32 = 16.0;
const HUD_SURFACE_PADDING: f32 = 8.0;

/// Application chrome is anchored to the drawable window, never to the match HUD.
pub(crate) fn global_settings_rect(window_width: f32) -> ScreenRect {
    ScreenRect {
        x: window_width - GLOBAL_SETTINGS_MARGIN - GLOBAL_SETTINGS_SIZE,
        y: GLOBAL_SETTINGS_MARGIN,
        w: GLOBAL_SETTINGS_SIZE,
        h: GLOBAL_SETTINGS_SIZE,
    }
}

/// 同格/相邻触区歧义时的编号选择面板；渲染和命中使用同一组矩形。
pub(crate) fn swap_piece_picker_rects(
    layout: GameLayout,
    count: usize,
) -> (ScreenRect, Vec<ScreenRect>) {
    let width = layout.board.w.clamp(240.0, 360.0);
    let columns = 3;
    let rows = count.div_ceil(columns);
    let height = 44.0 + rows as f32 * 56.0;
    let panel = ScreenRect {
        x: (layout.board.center().x - width * 0.5).max(8.0),
        y: (layout.board.center().y - height * 0.5).max(72.0),
        w: width,
        h: height,
    };
    let cell_width = (width - 32.0) / columns as f32;
    let cells = (0..count)
        .map(|i| ScreenRect {
            x: panel.x + 8.0 + (i % columns) as f32 * (cell_width + 8.0),
            y: panel.y + 36.0 + (i / columns) as f32 * 56.0,
            w: cell_width,
            h: 48.0,
        })
        .collect();
    (panel, cells)
}

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct ScreenRect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl ScreenRect {
    pub fn contains(self, p: Vec2) -> bool {
        p.x >= self.x && p.x <= self.x + self.w && p.y >= self.y && p.y <= self.y + self.h
    }

    pub fn overlaps(self, other: Self) -> bool {
        self.x < other.x + other.w
            && self.x + self.w > other.x
            && self.y < other.y + other.h
            && self.y + self.h > other.y
    }

    pub fn expanded(self, amount: f32) -> Self {
        Self {
            x: self.x - amount,
            y: self.y - amount,
            w: self.w + amount * 2.0,
            h: self.h + amount * 2.0,
        }
    }

    pub fn center(self) -> Vec2 {
        Vec2::new(self.x + self.w * 0.5, self.y + self.h * 0.5)
    }
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct GameLayout {
    pub board: ScreenRect,
    pub panel: ScreenRect,
    pub status: ScreenRect,
    pub settings: ScreenRect,
    pub skills: ScreenRect,
    pub charge: ScreenRect,
    /// Recent event / skill explanation share this space, never the skill buttons.
    pub context: ScreenRect,
    pub primary: ScreenRect,
    pub log_toggle: ScreenRect,
    /// Opening history replaces secondary controls, leaving the board and action accessible.
    pub log: ScreenRect,
    pub columns: usize,
    pub card_height: f32,
}

/// Card-local geometry shared by the visible icon, name and charge badge.
#[derive(Clone, Copy, Debug)]
pub(crate) struct SkillCardLayout {
    pub icon: ScreenRect,
    pub name: ScreenRect,
    pub badge: ScreenRect,
    pub mark: ScreenRect,
    pub name_font_size: f32,
    pub badge_font_size: f32,
}

impl SkillCardLayout {
    pub fn new(width: f32, height: f32) -> Self {
        let compact = height < 80.0;
        let badge_size = if height >= 92.0 { 20.0 } else { 18.0 };
        let icon_size = if compact {
            32.0
        } else if height >= 92.0 {
            44.0
        } else if height >= 84.0 {
            40.0
        } else {
            36.0
        };
        let icon = ScreenRect {
            x: if compact {
                6.0
            } else {
                (width - icon_size) * 0.5
            },
            y: if compact {
                (height - icon_size) * 0.5
            } else {
                badge_size + 2.0
            },
            w: icon_size,
            h: icon_size,
        };
        let name = if compact {
            ScreenRect {
                x: 42.0,
                y: 28.0,
                w: width - 46.0,
                h: 16.0,
            }
        } else {
            ScreenRect {
                x: 2.0,
                y: icon.y + icon.h + 4.0,
                w: width - 4.0,
                h: 16.0,
            }
        };
        Self {
            icon,
            name,
            badge: ScreenRect {
                x: width - badge_size - 2.0,
                y: 0.0,
                w: badge_size,
                h: badge_size,
            },
            mark: ScreenRect {
                x: if compact { 42.0 } else { 4.0 },
                y: 2.0,
                w: 16.0,
                h: 16.0,
            },
            name_font_size: if compact || width < 72.0 { 12.0 } else { 13.0 },
            badge_font_size: if badge_size >= 20.0 { 12.0 } else { 11.0 },
        }
    }
}

impl GameLayout {
    pub fn new(width: f32, height: f32, profile: DeviceProfile) -> Self {
        let side = profile.hud_layout == HudLayoutMode::SidePanel;
        let margin = 16.0;
        let gap = 8.0;
        let settings = global_settings_rect(width);
        let (board, panel, status, columns, card_height) = if side {
            let panel_width = (width * 0.25).clamp(280.0, 320.0).min(width * 0.48);
            let board_size = (width - panel_width - 24.0 - margin * 2.0)
                .min(height - 96.0)
                .clamp(120.0, 960.0);
            let group_x = (width - board_size - 24.0 - panel_width) * 0.5;
            let board = ScreenRect {
                x: group_x,
                y: (height - board_size) * 0.5,
                w: board_size,
                h: board_size,
            };
            // Account for the *visible* background, which extends beyond content.
            let panel_y = settings.y + settings.h + GLOBAL_SETTINGS_GAP + HUD_SURFACE_PADDING;
            let panel = ScreenRect {
                x: group_x + board_size + 24.0,
                y: panel_y,
                w: panel_width,
                h: height - panel_y - margin,
            };
            let status = ScreenRect {
                x: panel.x,
                y: if height < 540.0 { settings.y } else { panel.y },
                w: if height < 540.0 {
                    (settings.x - GLOBAL_SETTINGS_GAP - panel.x).min(panel.w)
                } else {
                    panel.w
                },
                h: if height < 620.0 { 48.0 } else { 64.0 },
            };
            (
                board,
                panel,
                status,
                3,
                if height < 420.0 {
                    54.0
                } else if height < 540.0 {
                    80.0
                } else {
                    92.0
                },
            )
        } else {
            // Reserve the complete dock and both hangar badge rows before sizing the board.
            let dock_height = if height < 740.0 { 248.0 } else { 284.0 };
            let board_size = (width - margin * 2.0)
                .min(height - 128.0 - dock_height)
                .clamp(120.0, 960.0);
            let board = ScreenRect {
                x: (width - board_size) * 0.5,
                y: 96.0,
                w: board_size,
                h: board_size,
            };
            let panel_width = (width - margin * 2.0).min(960.0);
            let panel = ScreenRect {
                x: (width - panel_width) * 0.5,
                y: board.y + board.h + 40.0,
                w: panel_width,
                h: height - margin - board.y - board.h - 40.0,
            };
            let status = ScreenRect {
                x: margin,
                y: settings.y,
                w: (settings.x - GLOBAL_SETTINGS_GAP - margin).max(120.0),
                h: 48.0,
            };
            (
                board,
                panel,
                status,
                5,
                if height < 740.0 { 84.0 } else { 92.0 },
            )
        };
        let skill_width = if side { panel.w } else { panel.w.min(512.0) };
        let skills = ScreenRect {
            x: panel.x + (panel.w - skill_width) * 0.5,
            y: if side && height >= 540.0 {
                status.y + status.h + gap
            } else {
                panel.y
            },
            w: skill_width,
            h: (5_usize.div_ceil(columns) as f32) * card_height
                + (5_usize.div_ceil(columns) - 1) as f32 * gap,
        };
        let charge = ScreenRect {
            x: skills.x,
            y: skills.y + skills.h + gap,
            w: skills.w,
            h: 20.0,
        };
        let action_y = height - margin - 48.0;
        let log_toggle = ScreenRect {
            x: panel.x,
            y: action_y,
            w: 80.0,
            h: 48.0,
        };
        let primary = ScreenRect {
            x: panel.x + 88.0,
            y: action_y,
            w: panel.w - 88.0,
            h: 48.0,
        };
        let context = ScreenRect {
            x: panel.x,
            y: charge.y + charge.h + gap,
            w: panel.w,
            h: (action_y - gap - charge.y - charge.h - gap).max(0.0),
        };
        let log = ScreenRect {
            x: panel.x,
            y: skills.y,
            w: panel.w,
            h: action_y - gap - skills.y,
        };
        Self {
            board,
            panel,
            status,
            settings,
            skills,
            charge,
            context,
            primary,
            log_toggle,
            log,
            columns,
            card_height,
        }
    }

    pub fn skill(self, index: usize) -> ScreenRect {
        let w = (self.skills.w - (self.columns - 1) as f32 * 8.0) / self.columns as f32;
        ScreenRect {
            x: self.skills.x + (index % self.columns) as f32 * (w + 8.0),
            y: self.skills.y + (index / self.columns) as f32 * (self.card_height + 8.0),
            w,
            h: self.card_height,
        }
    }

    pub fn surface(self) -> ScreenRect {
        self.panel.expanded(HUD_SURFACE_PADDING)
    }

    /// Keep this hit target independent of the HUD action button on resize/rotation.
    pub fn center_dice_roll(self) -> ScreenRect {
        // Four logical pixels of padding also cover the halo's breathing animation.
        let size =
            (CENTER_DICE_HALO_OUTER_RADIUS * 2.0 * self.board.w / BOARD_WORLD_SIZE + 8.0).max(64.0);
        let center = self.board.center();
        ScreenRect {
            x: center.x - size * 0.5,
            y: center.y - size * 0.5,
            w: size,
            h: size,
        }
    }

    pub fn fps(self) -> ScreenRect {
        ScreenRect {
            x: self.board.center().x - 32.0,
            y: if self.panel.x > self.board.x + self.board.w {
                8.0
            } else {
                68.0
            },
            w: 64.0,
            h: 24.0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn skill_icon_name_count_and_state_mark_have_separate_visible_bounds() {
        for (w, h) in [
            (360., 640.),
            (390., 844.),
            (600., 739.),
            (600., 740.),
            (720., 1280.),
            (981., 998.),
            (640., 360.),
            (900., 419.),
            (900., 420.),
            (900., 539.),
            (900., 540.),
            (1024., 600.),
            (1280., 720.),
            (2560., 1600.),
        ] {
            let layout = GameLayout::new(w, h, DeviceProfile::from_window_size(w, h));
            assert!(
                layout.context.h >= 48.0,
                "{w}x{h}: guidance must remain readable"
            );
            for i in 0..5 {
                let rect = layout.skill(i);
                let card = SkillCardLayout::new(rect.w, rect.h);
                let visible = [card.icon, card.name, card.badge, card.mark];
                for (j, item) in visible.iter().enumerate() {
                    assert!(
                        item.x >= 0.0
                            && item.y >= 0.0
                            && item.x + item.w <= rect.w + 0.01
                            && item.y + item.h <= rect.h,
                        "{w}x{h}: {item:?} exceeds {rect:?}"
                    );
                    for other in &visible[j + 1..] {
                        assert!(
                            !item.overlaps(*other),
                            "{w}x{h}: {item:?} overlaps {other:?}"
                        );
                    }
                }
                assert!(card.icon.w >= 32.0 && card.icon.w >= card.name_font_size * 2.5);
                if h >= 540.0 {
                    assert!(card.icon.w >= 40.0);
                }
                assert!((12.0..=13.0).contains(&card.name_font_size));
            }
        }
    }

    #[test]
    fn skill_grid_keeps_order_equal_cards_and_does_not_stretch_on_large_portrait() {
        for (w, h) in [(360., 640.), (720., 1280.), (981., 998.), (1280., 720.)] {
            let layout = GameLayout::new(w, h, DeviceProfile::from_window_size(w, h));
            let first = layout.skill(0);
            for i in 0..5 {
                assert_eq!((layout.skill(i).w, layout.skill(i).h), (first.w, first.h));
            }
            if w > h {
                assert_eq!(layout.columns, 3);
                assert_eq!(layout.skill(0).y, layout.skill(2).y);
                assert_eq!(layout.skill(3).x, first.x);
                assert_eq!(layout.skill(3).y, layout.skill(4).y);
            } else {
                assert_eq!(layout.columns, 5);
                assert_eq!(layout.skill(4).y, first.y);
                assert!(first.w <= 96.0);
                assert!((layout.skills.center().x - layout.panel.center().x).abs() < 0.01);
            }
        }
    }

    #[test]
    fn global_settings_clear_visible_hud_edges_at_every_breakpoint() {
        for (w, h) in [
            (360., 640.),
            (390., 844.),
            (600., 960.),
            (720., 1280.),
            (640., 360.),
            (900., 539.),
            (900., 540.),
            (1024., 619.),
            (1024., 620.),
            (600., 739.),
            (600., 740.),
            (1280., 720.),
            (1920., 1080.),
            (2560., 1600.),
        ] {
            let layout = GameLayout::new(w, h, DeviceProfile::from_window_size(w, h));
            let button = layout.settings;
            assert_eq!(button.x + button.w, w - 16.0);
            assert_eq!(button.y, 16.0);
            assert_eq!((button.w, button.h), (48.0, 48.0));
            for visible in [layout.surface(), layout.status, layout.board, layout.fps()] {
                assert!(
                    !button.expanded(16.0 - 0.001).overlaps(visible),
                    "{w}x{h}: {visible:?}"
                );
            }
            if w > h && w >= 640.0 {
                assert_eq!(layout.surface().y - button.y - button.h, 16.0);
                if h < 540.0 {
                    assert!(!layout.status.overlaps(layout.surface()));
                    assert_eq!(layout.skills.y, layout.panel.y);
                }
            }
        }
    }

    #[test]
    fn center_roll_target_tracks_board_and_stays_separate_from_primary_action() {
        for (w, h) in [
            (360., 640.),
            (390., 844.),
            (720., 1280.),
            (640., 360.),
            (900., 539.),
            (900., 540.),
            (1024., 619.),
            (1024., 620.),
            (600., 739.),
            (600., 740.),
            (1280., 720.),
            (1920., 1080.),
        ] {
            let layout = GameLayout::new(w, h, DeviceProfile::from_window_size(w, h));
            let hit = layout.center_dice_roll();
            assert_eq!(hit.center(), layout.board.center());
            assert!(hit.w >= 64.0 && hit.h >= 64.0);
            assert!(layout.board.contains(Vec2::new(hit.x, hit.y)));
            assert!(
                layout
                    .board
                    .contains(Vec2::new(hit.x + hit.w, hit.y + hit.h))
            );
            assert!(!hit.overlaps(layout.primary));
        }
    }

    #[test]
    fn board_controls_and_context_never_overlap_across_supported_windows() {
        for (w, h) in [
            (360., 640.),
            (390., 844.),
            (600., 960.),
            (720., 1280.),
            (640., 360.),
            (900., 539.),
            (900., 540.),
            (1024., 619.),
            (1024., 620.),
            (600., 739.),
            (600., 740.),
            (1024., 600.),
            (1280., 720.),
            (1280., 800.),
            (1920., 1080.),
            (2560., 1600.),
        ] {
            let layout = GameLayout::new(w, h, DeviceProfile::from_window_size(w, h));
            let mut rects = vec![
                layout.board,
                layout.status,
                layout.settings,
                layout.charge,
                layout.context,
                layout.primary,
                layout.log_toggle,
            ];
            rects.extend((0..5).map(|i| layout.skill(i)));
            for (i, a) in rects.iter().enumerate() {
                assert!(
                    a.x >= 0.0 && a.y >= 0.0 && a.x + a.w <= w + 0.01 && a.y + a.h <= h + 0.01,
                    "{w}x{h}: {a:?}"
                );
                for b in &rects[i + 1..] {
                    assert!(!a.overlaps(*b), "{w}x{h}: {a:?} overlaps {b:?}");
                }
            }
            assert!(!layout.log.overlaps(layout.board));
            assert!(layout.context.h >= 48.0, "{w}x{h}: context is too small");
            for i in 0..5 {
                let r = layout.skill(i);
                assert!(r.w >= 48.0 && r.h >= 48.0);
            }
        }
    }

    #[test]
    fn landscape_content_has_stable_gutter_and_large_board_can_grow() {
        for (w, h) in [(1024., 600.), (1280., 720.), (1920., 1080.)] {
            let l = GameLayout::new(w, h, DeviceProfile::from_window_size(w, h));
            assert!((l.panel.x - l.board.x - l.board.w - 24.0).abs() < 0.01);
        }
        assert!(
            GameLayout::new(1920., 1080., DeviceProfile::from_window_size(1920., 1080.))
                .board
                .w
                > 683.0
        );
    }
}
