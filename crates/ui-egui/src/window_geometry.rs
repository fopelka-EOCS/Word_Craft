//! The main window's size, position and maximized state, saved with the UI preferences so the
//! next launch opens the window where the user left it (#23).
//!
//! Sizes and positions are in the units a window opens with: what [`egui::ViewportBuilder`]
//! takes before the app sets its Interface size (egui's zoom factor), so the system's display
//! scale but not the Interface size. [`unzoomed`] turns what [`egui::ViewportInfo`] reports (egui
//! points, which the Interface size scales) into them.

use serde::{Deserialize, Serialize};

/// The smallest window restored: the desktop app's minimum inner size.
pub const MIN_SIZE: egui::Vec2 = egui::vec2(760.0, 480.0);
/// Larger saved sizes are corrupt. eframe also shrinks the start size to the largest monitor.
const MAX_SIZE: egui::Vec2 = egui::vec2(16384.0, 16384.0);
/// Saved positions farther from the origin than this are corrupt.
const MAX_COORD: f32 = 32768.0;
/// Room a window leaves on each side of its monitor for the taskbar, the Dock or a desktop panel,
/// which the monitor size (all egui reports) includes.
const SYSTEM_BAR: egui::Vec2 = egui::vec2(48.0, 48.0);

/// The window as it was when the app last closed.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct WindowGeometry {
    /// Top-left corner: of the frame, or of the content on macOS (where winit places a new
    /// window by it). `None` where the platform doesn't report it (Wayland).
    pub position: Option<[f32; 2]>,
    /// Content size of the un-maximized window.
    pub width: f32,
    pub height: f32,
    pub maximized: bool,
    /// Size of the monitor the window was on, to notice when that monitor is gone.
    pub monitor: Option<[f32; 2]>,
}

impl WindowGeometry {
    /// The geometry to save, given the last saved one and the window as it is now. While the
    /// window is minimized, full screen or maximized, the last un-maximized size and position are
    /// kept, so un-maximizing after the next launch returns to them.
    pub fn track(prev: Option<Self>, info: &egui::ViewportInfo, content_size: egui::Vec2) -> Option<Self> {
        if info.minimized == Some(true) || info.fullscreen == Some(true) {
            return prev;
        }
        // egui reads the maximized state on macOS only when the window opens (egui#3494), so a
        // window that opened zoomed would stay "maximized" for good. There the zoomed frame is
        // saved like any other size instead.
        let maximized = info.maximized == Some(true) && !cfg!(target_os = "macos");
        if maximized && let Some(prev) = prev {
            return Some(Self { maximized, ..prev });
        }
        let rect = if cfg!(target_os = "macos") { info.inner_rect } else { info.outer_rect };
        let size = info.inner_rect.map_or(content_size, |r| r.size());
        let position = rect.map(|r| [r.min.x, r.min.y]);
        let monitor = info.monitor_size.map(|s| [s.x, s.y]);
        Self { position, width: size.x, height: size.y, maximized, monitor }.sanitized().or(prev)
    }

    /// `None` for a size that isn't positive and finite. Otherwise the size is clamped to
    /// [`MIN_SIZE`]..=16384, and a position or monitor size that can't be real is dropped.
    pub fn sanitized(self) -> Option<Self> {
        let size = egui::vec2(self.width, self.height);
        if !size.is_finite() || size.x <= 0.0 || size.y <= 0.0 {
            return None;
        }
        let size = size.clamp(MIN_SIZE, MAX_SIZE);
        let position = self.position.filter(|p| p.iter().all(|c| c.is_finite() && c.abs() <= MAX_COORD));
        let monitor = self.monitor.filter(|m| m.iter().all(|c| c.is_finite() && *c > 0.0 && *c <= MAX_SIZE.x));
        Some(Self { position, width: size.x, height: size.y, maximized: self.maximized, monitor })
    }

    /// Whether the window is maximized after it has opened rather than as it opens. On Windows,
    /// winit shows a window the moment it creates it maximized, even one that is meant to stay
    /// hidden until the first frame (eframe's white-flash guard), so a maximized window flashed
    /// on and off about four times at start-up. There it opens at its normal size, hidden, and the
    /// first frame maximizes it ([`Self::maximize_after_open`]).
    pub fn maximizes_after_open(self) -> bool {
        self.maximized && cfg!(windows)
    }

    /// The command that maximizes the window after it opened, if [`Self::maximizes_after_open`].
    pub fn maximize_after_open(self) -> Option<egui::ViewportCommand> {
        self.maximizes_after_open().then_some(egui::ViewportCommand::Maximized(true))
    }

    /// Open the window at this size, position and maximized state.
    pub fn apply(self, builder: egui::ViewportBuilder) -> egui::ViewportBuilder {
        let builder = builder.with_inner_size([self.width, self.height]).with_maximized(self.maximized && !self.maximizes_after_open());
        match self.position {
            Some(p) => builder.with_position(p),
            None => builder,
        }
    }

    /// After a restore: where to move the window when the monitor it now reports differs from the
    /// one it was saved on (an external monitor was unplugged, say), so a window restored to a
    /// position no monitor covers any more doesn't open off screen. Centred on the primary
    /// monitor, whose top-left corner is the origin. A maximized window fills whichever monitor
    /// the system picks, so it stays put.
    pub fn rescue_position(&self, info: &egui::ViewportInfo) -> Option<egui::Pos2> {
        if self.maximized || self.position.is_none() {
            return None;
        }
        let saved = egui::Vec2::from(self.monitor?);
        let now = info.monitor_size.filter(|s| s.is_finite() && s.x > 0.0 && s.y > 0.0)?;
        if (now - saved).length() <= 1.0 {
            return None;
        }
        let free = now - egui::vec2(self.width, self.height);
        Some(egui::pos2((free.x / 2.0).max(0.0), (free.y / 2.0).max(0.0)))
    }

    /// Once the monitor size is known: a smaller size and a centred position for a window that
    /// doesn't fit on its monitor, frame included (#1215). eframe shrinks the start size to the
    /// monitor's, but not by the frame, the title bar or the taskbar, so on a laptop screen the
    /// default window reached past the screen's edge, and the end of the scroll bar with it: a
    /// drag of the scroll bar stopped where the pointer met the edge. A window the system placed
    /// (no position here) is on the primary monitor, whose top-left corner is the origin, so its
    /// position counts too; a restored one may be on another monitor, so only its size does. A
    /// maximized window fills its monitor already.
    pub fn fit_on_monitor(&self, info: &egui::ViewportInfo) -> Option<(egui::Pos2, egui::Vec2)> {
        if self.maximized || info.maximized == Some(true) || info.fullscreen == Some(true) || info.minimized == Some(true) {
            return None;
        }
        let monitor = info.monitor_size.filter(|s| s.is_finite() && s.x > 0.0 && s.y > 0.0)?;
        let inner = info.inner_rect.filter(|r| r.is_finite())?;
        let outer = info.outer_rect.filter(|r| r.is_finite()).unwrap_or(inner);
        let frame = (outer.size() - inner.size()).max(egui::Vec2::ZERO);
        // A window restored on the monitor it was saved on keeps the size and place it was left
        // at, as long as what shows of it fits on that monitor: the system bars' room below is a
        // guess, and a window sized to fill a small screen (1600 × 900) is past it, so it was
        // shrunk and centred at every launch (#2828). Browsers do the same (Chromium's window
        // sizer only shrinks a saved window when the screen's work area changed). What shows is
        // the frame less its side and bottom borders, which Windows draws mostly invisible, past
        // the edge of a window dragged to the screen's edges.
        let border = frame.x / 2.0;
        let shown = egui::vec2(outer.width() - frame.x, outer.height() - border);
        let same_monitor = self.monitor.is_some_and(|m| (egui::Vec2::from(m) - monitor).length() <= 1.0);
        if self.position.is_some() && same_monitor && shown.x <= monitor.x && shown.y <= monitor.y {
            return None;
        }
        let room = monitor - 2.0 * SYSTEM_BAR;
        let too_big = outer.width() > room.x || outer.height() > room.y;
        let edge = monitor - SYSTEM_BAR;
        let past_edge = self.position.is_none() && (outer.max.x > edge.x || outer.max.y > edge.y);
        if !too_big && !past_edge {
            return None;
        }
        let size = (room - frame).min(inner.size()).max(MIN_SIZE);
        let free = monitor - (size + frame);
        Some((egui::pos2((free.x / 2.0).max(0.0), (free.y / 2.0).max(0.0)), size))
    }
}

/// Checks the restored geometry in `pending` once, and only once the monitor size is known: while
/// `info` doesn't report a usable one, `pending` is left in place for a later frame. Then it is taken
/// and [`WindowGeometry::rescue_position`] decides where to move the window, if anywhere.
pub fn take_rescue(pending: &mut Option<WindowGeometry>, info: &egui::ViewportInfo) -> Option<egui::Pos2> {
    info.monitor_size.filter(|s| s.is_finite() && s.x > 0.0 && s.y > 0.0)?;
    pending.take()?.rescue_position(info)
}

/// The window as `info` reports it (egui points, at `pixels_per_point` screen pixels each), in the
/// units a window opens with, and the Interface size (egui's zoom factor) that was taken out. The
/// window opens before the app sets its Interface size, so a window saved in points reopened
/// scaled by the inverse of that size: with 125 % on a 100 % display it shrank by a fifth
/// at every launch, and its monitor, saved in points too, looked like another one, so it was moved
/// to the middle (#2828). Only what [`WindowGeometry`] reads is kept.
pub fn unzoomed(info: &egui::ViewportInfo, pixels_per_point: f32) -> (egui::ViewportInfo, f32) {
    let zoom = info.native_pixels_per_point.map(|native| pixels_per_point / native).filter(|z| z.is_finite() && *z > 0.0).unwrap_or(1.0);
    let unzoomed = egui::ViewportInfo {
        native_pixels_per_point: info.native_pixels_per_point,
        monitor_size: info.monitor_size.map(|s| s * zoom),
        inner_rect: info.inner_rect.map(|r| r * zoom),
        outer_rect: info.outer_rect.map(|r| r * zoom),
        minimized: info.minimized,
        maximized: info.maximized,
        fullscreen: info.fullscreen,
        ..Default::default()
    };
    (unzoomed, zoom)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn info(rect: egui::Rect, monitor: egui::Vec2) -> egui::ViewportInfo {
        egui::ViewportInfo { inner_rect: Some(rect), outer_rect: Some(rect), monitor_size: Some(monitor), ..Default::default() }
    }

    fn saved() -> WindowGeometry {
        WindowGeometry { position: Some([100.0, 50.0]), width: 1200.0, height: 800.0, maximized: false, monitor: Some([1920.0, 1080.0]) }
    }

    #[test]
    fn records_the_window_and_restores_it() {
        let rect = egui::Rect::from_min_size(egui::pos2(100.0, 50.0), egui::vec2(1200.0, 800.0));
        let g = WindowGeometry::track(None, &info(rect, egui::vec2(1920.0, 1080.0)), egui::Vec2::ZERO);
        assert_eq!(g, Some(saved()));
        let b = saved().apply(egui::ViewportBuilder::default());
        assert_eq!(b.inner_size, Some(egui::vec2(1200.0, 800.0)));
        assert_eq!(b.position, Some(egui::pos2(100.0, 50.0)));
        assert_eq!(b.maximized, Some(false));
    }

    #[test]
    fn a_maximized_window_opens_normal_on_windows_and_is_maximized_by_the_first_frame() {
        let g = WindowGeometry { maximized: true, ..saved() };
        let b = g.apply(egui::ViewportBuilder::default());
        // Normal size and position either way, so un-maximizing returns to them.
        assert_eq!(b.inner_size, Some(egui::vec2(1200.0, 800.0)));
        assert_eq!(b.position, Some(egui::pos2(100.0, 50.0)));
        if cfg!(windows) {
            assert_eq!(b.maximized, Some(false), "winit would show the hidden window while creating it maximized");
            assert_eq!(g.maximize_after_open(), Some(egui::ViewportCommand::Maximized(true)));
        } else {
            assert_eq!(b.maximized, Some(true));
            assert_eq!(g.maximize_after_open(), None);
        }
        assert_eq!(saved().maximize_after_open(), None, "a normal window needs no command");
    }

    #[test]
    fn maximized_keeps_the_normal_size_and_position() {
        let full = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1920.0, 1040.0));
        let mut i = info(full, egui::vec2(1920.0, 1080.0));
        i.maximized = Some(true);
        let g = WindowGeometry::track(Some(saved()), &i, egui::Vec2::ZERO);
        if cfg!(target_os = "macos") {
            // The zoomed frame is saved as the size; the window never reopens "maximized".
            assert_eq!(g.map(|g| (g.position, g.width, g.height, g.maximized)), Some((Some([0.0, 0.0]), 1920.0, 1040.0, false)));
        } else {
            assert_eq!(g, Some(WindowGeometry { maximized: true, ..saved() }));
        }
        // Minimized or full screen: nothing changes.
        i.maximized = Some(false);
        i.minimized = Some(true);
        assert_eq!(WindowGeometry::track(Some(saved()), &i, egui::Vec2::ZERO), Some(saved()));
        i.minimized = None;
        i.fullscreen = Some(true);
        assert_eq!(WindowGeometry::track(Some(saved()), &i, egui::Vec2::ZERO), Some(saved()));
    }

    #[test]
    fn without_a_position_the_content_size_is_kept() {
        // Wayland reports neither rect.
        let i = egui::ViewportInfo::default();
        let g = WindowGeometry::track(None, &i, egui::vec2(1000.0, 700.0));
        assert_eq!(g, Some(WindowGeometry { position: None, width: 1000.0, height: 700.0, maximized: false, monitor: None }));
        assert_eq!(g.map(|g| g.apply(egui::ViewportBuilder::default()).position), Some(None));
    }

    #[test]
    fn insane_values_are_clamped_or_dropped() {
        let zero = WindowGeometry { width: 0.0, height: 800.0, ..saved() };
        assert_eq!(zero.sanitized(), None);
        assert_eq!(WindowGeometry::default().sanitized(), None);
        let nan = WindowGeometry { width: f32::NAN, ..saved() };
        assert_eq!(nan.sanitized(), None);
        let tiny = WindowGeometry { width: 10.0, height: 1.0e9, ..saved() }.sanitized();
        assert_eq!(tiny.map(|g| (g.width, g.height)), Some((MIN_SIZE.x, 16384.0)));
        let far = WindowGeometry { position: Some([1.0e7, 0.0]), monitor: Some([f32::INFINITY, 1080.0]), ..saved() }.sanitized();
        assert_eq!(far.map(|g| (g.position, g.monitor)), Some((None, None)));
        assert_eq!(saved().sanitized(), Some(saved()));
    }

    #[test]
    fn a_window_whose_monitor_is_gone_is_moved_on_screen() {
        let on = egui::Rect::from_min_size(egui::pos2(2000.0, 50.0), egui::vec2(1200.0, 800.0));
        // Same monitor: stays where it was.
        assert_eq!(saved().rescue_position(&info(on, egui::vec2(1920.0, 1080.0))), None);
        // A different (smaller) monitor now: centred on it.
        assert_eq!(saved().rescue_position(&info(on, egui::vec2(1600.0, 900.0))), Some(egui::pos2(200.0, 50.0)));
        // Larger than that monitor: its top-left corner.
        assert_eq!(saved().rescue_position(&info(on, egui::vec2(1024.0, 768.0))), Some(egui::Pos2::ZERO));
        // Nothing to compare, or no saved position: no move.
        assert_eq!(WindowGeometry { monitor: None, ..saved() }.rescue_position(&info(on, egui::vec2(1600.0, 900.0))), None);
        assert_eq!(WindowGeometry { position: None, ..saved() }.rescue_position(&info(on, egui::vec2(1600.0, 900.0))), None);
        assert_eq!(WindowGeometry { maximized: true, ..saved() }.rescue_position(&info(on, egui::vec2(1600.0, 900.0))), None);
        assert_eq!(saved().rescue_position(&egui::ViewportInfo::default()), None);
    }

    #[test]
    fn the_rescue_waits_for_the_monitor_size() {
        let on = egui::Rect::from_min_size(egui::pos2(2000.0, 50.0), egui::vec2(1200.0, 800.0));
        // The first frame doesn't know the monitor yet: nothing happens and the geometry is kept.
        let mut pending = Some(saved());
        assert_eq!(take_rescue(&mut pending, &egui::ViewportInfo::default()), None);
        assert_eq!(pending, Some(saved()));
        // A later frame reports a different monitor: centred on it, and the check is done.
        assert_eq!(take_rescue(&mut pending, &info(on, egui::vec2(1600.0, 900.0))), Some(egui::pos2(200.0, 50.0)));
        assert_eq!(pending, None);
        assert_eq!(take_rescue(&mut pending, &info(on, egui::vec2(1024.0, 768.0))), None);
        // The same monitor: no move, and the check is done as well.
        let mut pending = Some(saved());
        assert_eq!(take_rescue(&mut pending, &info(on, egui::vec2(1920.0, 1080.0))), None);
        assert_eq!(pending, None);
    }

    #[test]
    fn survives_the_prefs_round_trip() {
        let ui = crate::UiState { window: Some(saved()), ..Default::default() };
        let json = serde_json::to_string(&ui).unwrap();
        let back: crate::UiState = serde_json::from_str(&json).unwrap();
        assert_eq!(back.window, Some(saved()));
        // Prefs written before #23 have no window.
        let old: crate::UiState = serde_json::from_str(r#"{"tab":"Insert"}"#).unwrap();
        assert_eq!(old.window, None);
        assert_eq!(old.tab, "Insert");
    }

    /// A window opened with its frame's top-left corner at `at`: 8 pt borders, a 31 pt title bar.
    fn opened(at: egui::Pos2, content: egui::Vec2, monitor: egui::Vec2) -> egui::ViewportInfo {
        egui::ViewportInfo {
            inner_rect: Some(egui::Rect::from_min_size(at + egui::vec2(8.0, 31.0), content)),
            outer_rect: Some(egui::Rect::from_min_size(at, content + egui::vec2(16.0, 39.0))),
            monitor_size: Some(monitor),
            ..Default::default()
        }
    }

    #[test]
    fn a_window_too_big_for_its_screen_shrinks_to_fit() {
        let system = WindowGeometry::default();
        let at = egui::pos2(78.0, 78.0);
        // A 1920 × 1080 laptop screen at 125 %: eframe gave the default window the screen's height,
        // so the frame and the title bar push its bottom, and the scroll bar's end, off screen.
        let laptop = egui::vec2(1536.0, 864.0);
        let fitted = Some((egui::pos2(48.0, 48.0), egui::vec2(1424.0, 729.0)));
        assert_eq!(system.fit_on_monitor(&opened(at, egui::vec2(1440.0, 864.0), laptop)), fitted);
        // The same size restored from an earlier launch shrinks too.
        assert_eq!(saved().fit_on_monitor(&opened(at, egui::vec2(1440.0, 864.0), laptop)), fitted);
        // Full HD: the default size fits, but where the system put it the bottom reaches into the
        // taskbar, so it is only centred.
        let full_hd = egui::vec2(1920.0, 1080.0);
        let start = egui::vec2(1440.0, 920.0);
        assert_eq!(system.fit_on_monitor(&opened(at, start, full_hd)), Some((egui::pos2(232.0, 60.5), start)));
        // A restored window keeps its place: it may be on another monitor (its origin isn't known).
        assert_eq!(saved().fit_on_monitor(&opened(at, start, full_hd)), None);
        assert_eq!(saved().fit_on_monitor(&opened(egui::pos2(2000.0, 78.0), start, full_hd)), None);
        // Room to spare: nothing moves.
        assert_eq!(system.fit_on_monitor(&opened(at, start, egui::vec2(2560.0, 1440.0))), None);
        // Once fitted, the window saved and restored on the next launch stays as it is.
        let again = WindowGeometry { position: Some([48.0, 48.0]), width: 1424.0, height: 729.0, ..saved() };
        assert_eq!(again.fit_on_monitor(&opened(egui::pos2(48.0, 48.0), egui::vec2(1424.0, 729.0), laptop)), None);
        // Never smaller than the minimum size, and never off the top-left corner.
        let tiny = egui::vec2(800.0, 600.0);
        assert_eq!(system.fit_on_monitor(&opened(at, start, tiny)), Some((egui::pos2(12.0, 40.5), MIN_SIZE)));
        // Maximized, full screen or minimized, or the monitor not known yet: nothing to do.
        let mut i = opened(at, egui::vec2(1440.0, 864.0), laptop);
        assert_eq!(WindowGeometry { maximized: true, ..saved() }.fit_on_monitor(&i), None);
        i.maximized = Some(true);
        assert_eq!(system.fit_on_monitor(&i), None);
        i.maximized = None;
        i.fullscreen = Some(true);
        assert_eq!(system.fit_on_monitor(&i), None);
        i.fullscreen = None;
        i.minimized = Some(true);
        assert_eq!(system.fit_on_monitor(&i), None);
        i.minimized = None;
        i.monitor_size = None;
        assert_eq!(system.fit_on_monitor(&i), None);
        // Wayland reports no rects.
        assert_eq!(system.fit_on_monitor(&egui::ViewportInfo { monitor_size: Some(laptop), ..Default::default() }), None);
    }

    /// The corner [`WindowGeometry::track`] saves: the frame's, or the content's on macOS.
    fn corner(info: &egui::ViewportInfo) -> Option<[f32; 2]> {
        let rect = if cfg!(target_os = "macos") { info.inner_rect } else { info.outer_rect };
        rect.map(|r| [r.min.x, r.min.y])
    }

    #[test]
    fn a_window_left_filling_a_small_screen_comes_back_as_it_was() {
        // 1600 × 900: the window dragged to the edges of the screen above a 48 pt taskbar, its
        // invisible side and bottom borders past them (#2828).
        let small = egui::vec2(1600.0, 900.0);
        let at = egui::pos2(-8.0, 0.0);
        let content = egui::vec2(1600.0, 813.0);
        let left = WindowGeometry::track(None, &opened(at, content, small), egui::Vec2::ZERO).unwrap();
        let position = corner(&opened(at, content, small));
        assert_eq!(left, WindowGeometry { position, width: 1600.0, height: 813.0, maximized: false, monitor: Some([1600.0, 900.0]) });
        // The next launch opens it there, and nothing shrinks or centres it.
        let b = left.apply(egui::ViewportBuilder::default());
        assert_eq!((b.position, b.inner_size), (position.map(egui::Pos2::from), Some(content)));
        assert_eq!(left.fit_on_monitor(&opened(at, content, small)), None);
        assert_eq!(left.rescue_position(&opened(at, content, small)), None);
        // Taller than the screen (eframe's default size, saved before #1215): still shrinks.
        let tall = WindowGeometry { height: 900.0, ..left };
        assert!(tall.fit_on_monitor(&opened(at, egui::vec2(1600.0, 900.0), small)).is_some());
        // Saved on another monitor, or without the monitor or the place it was left at: the room
        // for the system bars counts again.
        let elsewhere = WindowGeometry { monitor: Some([1920.0, 1080.0]), ..left };
        assert!(elsewhere.fit_on_monitor(&opened(at, content, small)).is_some());
        assert!(WindowGeometry { monitor: None, ..left }.fit_on_monitor(&opened(at, content, small)).is_some());
        assert!(WindowGeometry { position: None, ..left }.fit_on_monitor(&opened(at, content, small)).is_some());
        // The same monitor reported a fraction of a point off (a scale that doesn't divide it).
        assert_eq!(left.fit_on_monitor(&opened(at, content, egui::vec2(1600.4, 899.6))), None);
        // Wider than the screen by more than its invisible borders: shrinks.
        let wide = egui::vec2(1610.0, 813.0);
        assert!(WindowGeometry { width: 1610.0, ..left }.fit_on_monitor(&opened(at, wide, small)).is_some());
    }

    #[test]
    fn an_interface_size_other_than_auto_reopens_the_window_as_it_was() {
        // The window as it opens, before the app sets its Interface size, on a 100 % display.
        let screen = egui::vec2(1280.0, 1024.0);
        let at = egui::pos2(200.0, 100.0);
        let content = egui::vec2(884.0, 561.0);
        let opening = egui::ViewportInfo { native_pixels_per_point: Some(1.0), ..opened(at, content, screen) };
        // Interface size 200 %: egui reports the same window in points two screen pixels each.
        let zoomed = egui::ViewportInfo {
            inner_rect: opening.inner_rect.map(|r| r * 0.5),
            outer_rect: opening.outer_rect.map(|r| r * 0.5),
            monitor_size: opening.monitor_size.map(|s| s * 0.5),
            ..opening.clone()
        };
        let (info, zoom) = unzoomed(&zoomed, 2.0);
        assert_eq!(zoom, 2.0);
        let left = WindowGeometry::track(None, &info, egui::Vec2::ZERO).unwrap();
        let position = corner(&opening);
        assert_eq!(left, WindowGeometry { position, width: 884.0, height: 561.0, maximized: false, monitor: Some([1280.0, 1024.0]) });
        // The next launch opens it at that size and place, which the first frame (no Interface
        // size yet) neither moves nor shrinks, and the frames after it save again unchanged.
        let b = left.apply(egui::ViewportBuilder::default());
        assert_eq!((b.position, b.inner_size), (position.map(egui::Pos2::from), Some(content)));
        let (first, zoom) = unzoomed(&opening, 1.0);
        assert_eq!(zoom, 1.0);
        assert_eq!(left.rescue_position(&first), None);
        assert_eq!(left.fit_on_monitor(&first), None);
        assert_eq!(WindowGeometry::track(Some(left), &first, egui::Vec2::ZERO), Some(left));
        assert_eq!(WindowGeometry::track(Some(left), &info, egui::Vec2::ZERO), Some(left));
        // A window too big for its screen is fitted the same at any Interface size: the commands
        // take points, the fitted window divided by the Interface size.
        let big = egui::vec2(1400.0, 1000.0);
        let opening_big = egui::ViewportInfo { native_pixels_per_point: Some(1.0), ..opened(at, big, screen) };
        let zoomed_big = egui::ViewportInfo {
            inner_rect: opening_big.inner_rect.map(|r| r * 0.5),
            outer_rect: opening_big.outer_rect.map(|r| r * 0.5),
            monitor_size: opening_big.monitor_size.map(|s| s * 0.5),
            ..opening_big.clone()
        };
        let plain = WindowGeometry::default().fit_on_monitor(&opening_big);
        assert!(plain.is_some());
        let (info_big, zoom) = unzoomed(&zoomed_big, 2.0);
        assert_eq!(WindowGeometry::default().fit_on_monitor(&info_big), plain);
        assert_eq!(plain.map(|(p, s)| (p / zoom, s / zoom)), plain.map(|(p, s)| (p * 0.5, s * 0.5)));
        // The window's state comes through, so a maximized, full-screen or minimized window is
        // still left alone.
        let state = egui::ViewportInfo { maximized: Some(true), fullscreen: Some(false), minimized: Some(true), ..zoomed.clone() };
        let (kept, _) = unzoomed(&state, 2.0);
        assert_eq!((kept.maximized, kept.fullscreen, kept.minimized), (Some(true), Some(false), Some(true)));
        assert_eq!(WindowGeometry::track(Some(left), &kept, egui::Vec2::ZERO), Some(left));
        // No display scale reported, or nonsense: nothing is taken out.
        for (native, ppp) in [
            (None, 2.0),
            (Some(0.0), 2.0),
            (Some(-1.0), 2.0),
            (Some(1.0), f32::NAN),
            (Some(1.0), 0.0),
            (Some(f32::NAN), 2.0),
            (Some(1.0), f32::INFINITY),
        ] {
            let (same, zoom) = unzoomed(&egui::ViewportInfo { native_pixels_per_point: native, ..zoomed.clone() }, ppp);
            assert_eq!(
                (same.inner_rect, same.outer_rect, same.monitor_size, zoom),
                (zoomed.inner_rect, zoomed.outer_rect, zoomed.monitor_size, 1.0),
                "{native:?} {ppp}"
            );
        }
    }
}
