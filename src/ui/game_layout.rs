//! Shared logical-pixel geometry for rendering, camera projection and hit testing.
use bevy::prelude::*;

use crate::platform::{DeviceProfile, HudLayoutMode};

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

impl GameLayout {
    pub fn new(width: f32, height: f32, profile: DeviceProfile) -> Self {
        let side = profile.hud_layout == HudLayoutMode::SidePanel;
        let margin = 16.0;
        let gap = 8.0;
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
            let panel_y = if height < 540.0 { 8.0 } else { 64.0 };
            let panel = ScreenRect {
                x: group_x + board_size + 24.0,
                y: panel_y,
                w: panel_width,
                h: height - panel_y - margin,
            };
            let status = ScreenRect {
                x: panel.x,
                y: panel.y,
                w: if height < 540.0 {
                    panel.w - 112.0
                } else {
                    panel.w
                },
                h: if height < 620.0 { 48.0 } else { 64.0 },
            };
            (
                board,
                panel,
                status,
                if height < 540.0 { 3 } else { 2 },
                if height < 540.0 {
                    48.0
                } else if height < 620.0 {
                    56.0
                } else {
                    68.0
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
                y: 8.0,
                w: (width - 176.0).max(120.0),
                h: 48.0,
            };
            (
                board,
                panel,
                status,
                5,
                if width < 500.0 { 64.0 } else { 76.0 },
            )
        };
        let skills = ScreenRect {
            x: panel.x,
            y: if side {
                status.y + status.h + gap
            } else {
                panel.y
            },
            w: panel.w,
            h: (5_usize.div_ceil(columns) as f32) * card_height
                + (5_usize.div_ceil(columns) - 1) as f32 * gap,
        };
        let charge = ScreenRect {
            x: panel.x,
            y: skills.y + skills.h + gap,
            w: panel.w,
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
        let settings = ScreenRect {
            x: if side {
                panel.x + panel.w - 104.0
            } else {
                width - margin - 128.0
            },
            y: 8.0,
            w: if side { 104.0 } else { 128.0 },
            h: 48.0,
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

    pub fn fps(self) -> ScreenRect {
        ScreenRect {
            x: self.board.center().x - 32.0,
            y: if self.panel.x > self.board.x + self.board.w {
                8.0
            } else {
                60.0
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
