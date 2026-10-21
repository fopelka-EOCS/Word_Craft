//! Mirrored interface for right-to-left languages (Hebrew). Instead of every panel, menu and dialog
//! laying itself out twice, the whole window is mirrored as it is painted: an egui plugin flips
//! each shape of the finished frame about the window's centre, and flips the pointer on the way
//! in, so a click lands on what is drawn under it. Text keeps its glyphs: a label moves to the
//! mirrored place, so it lines up on the right, and the lines of a wrapped label are aligned right.
//! Icons drawn in code flip, as they do in right-to-left Word; pictures keep their orientation.
//!
//! What has its own direction moves without flipping:
//! - the document: the page canvas with its rulers (a document's paragraphs say which way they
//!   run);
//! - the text field being edited, whose caret and selection are drawn apart from its text.
//!
//! Popups placed from the page (context menu, mini toolbar, tooltips over the page) are mirrored
//! about the point they open from, so they open leftwards from it, as right-to-left menus do.
//!
//! Interface text is also put in visual order here ([`bidi_galley`]): egui has no bidirectional
//! layout, so labels and catalog entries everywhere else stay in logical order (what document
//! values, file names, the window title and accessibility labels need).

use std::ops::Range;
use std::sync::Arc;

use egui::epaint::text::{Galley, Row};
use egui::epaint::{ClippedShape, Shape};
use egui::layers::ShapeIdx;
use egui::{Context, Event, FullOutput, Id, LayerId, Order, Pos2, RawInput, Rect, Ui, Vec2};
use unicode_bidi::{BidiClass, BidiInfo, Level, bidi_class};

/// How the pointer goes from where it is on screen to where the widgets were laid out.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Mapping {
    /// On a popup placed from the page: mirrored about its axis (the sum `x + x'`).
    Mirrored(f32),
    /// On the text field being edited: moved back by this much, then by the letter drawn there.
    Field(f32),
    /// Anywhere else in the interface: flipped about the window's centre.
    Flipped,
    /// On the page: moved back by this much.
    Moved(f32),
}

/// The plugin; [`configure`] sets it up each frame.
#[derive(Default)]
pub struct Mirror {
    on: bool,
    /// The document canvas (logical coordinates), when it is on screen.
    canvas: Option<Rect>,
    /// Popups placed from document coordinates (their area ids).
    anchored: Vec<Id>,
    /// Where those popups were this frame (logical), with the axis they were mirrored about (as
    /// the sum `x + x'`), for the pointer.
    anchored_rects: Vec<(Rect, f32)>,
    /// The other menus, popups and windows on screen (logical): they're flipped with the
    /// interface, and the pointer over one goes to it even where it's drawn over the page.
    floating: Vec<Rect>,
    /// How many of the frame's shapes are those of menus, popups and windows. They come after
    /// the page and the panels (egui paints background layers first), and are never the page's
    /// even where they lie over it: a dialog's text box or check mark there is flipped too.
    floating_shapes: usize,
    /// The text field being edited (logical), for the pointer.
    field: Option<Rect>,
    field_id: Option<Id>,
    /// Its cursor, in characters: where the caret is and the selection's other end.
    field_cursor: Option<(usize, usize)>,
    /// A press on it held down, in characters by the letter drawn under the pointer (egui's own
    /// columns are off in a right-to-left word, see [`drop_phantoms`]): where it was pressed (none
    /// with shift, which extends the selection) and where the pointer is now.
    field_press: Option<(Option<usize>, usize)>,
    /// Where the glyphs of the field's right-to-left text were drawn last frame, for the pointer.
    field_rows: Vec<RowMap>,
    /// The window's horizontal extent, from the last frame.
    span: Option<(f32, f32)>,
    /// The pointer as last seen on screen: wheel scrolling over the page isn't flipped.
    pointer: Option<Pos2>,
    /// The mapping of the button held down, kept until it is released.
    drag: Option<Mapping>,
}

impl egui::Plugin for Mirror {
    fn debug_name(&self) -> &'static str {
        "wordcraft mirror"
    }

    fn input_hook(&mut self, _ctx: &Context, input: &mut RawInput) {
        if let Some(r) = input.screen_rect {
            self.span = Some((r.min.x, r.max.x));
        }
        let Some(span) = self.span.filter(|_| self.on) else { return };
        // Areas kept unflipped, with how far each moves: the field being edited and the page.
        let field = self.field.map(|f| (f, keep_shift(span, f)));
        let canvas = self.canvas.map(|c| (c, keep_shift(span, c)));
        let shown = |(r, d): (Rect, f32), p: Pos2| r.translate(Vec2::new(d, 0.0)).contains(p);
        let anchored = &self.anchored_rects;
        let floating = &self.floating;
        let rows = &self.field_rows;
        // Over a menu, popup or window other than the one holding `inside`?
        let on_floating = |p: Pos2, inside: Option<Rect>| {
            let q = Pos2::new(flip_x(span, p.x), p.y);
            floating.iter().any(|r| r.contains(q) && !inside.is_some_and(|i| r.contains_rect(i)))
        };
        let mapping_at = |p: Pos2| -> Mapping {
            if let Some((_, sum)) = anchored.iter().find(|(r, sum)| flip_rect((0.0, *sum), *r).contains(p)) {
                return Mapping::Mirrored(*sum);
            }
            // The field being edited (in a dialog too) is drawn moved, not flipped: so is the pointer.
            if let Some((_, d)) = field.filter(|&k| shown(k, p) && !on_floating(p, Some(k.0))) {
                return Mapping::Field(d);
            }
            if on_floating(p, None) {
                return Mapping::Flipped;
            }
            match canvas.filter(|&k| shown(k, p)) {
                Some((_, d)) => Mapping::Moved(d),
                None => Mapping::Flipped,
            }
        };
        let apply = |m: Mapping, p: Pos2| -> Pos2 {
            match m {
                Mapping::Mirrored(sum) => Pos2::new(sum - p.x, p.y),
                Mapping::Field(d) => {
                    let p = Pos2::new(p.x - d, p.y);
                    // Its right-to-left text is reordered too: the caret goes by the letter drawn there.
                    match rows.iter().find(|r| (r.top..r.bottom).contains(&p.y)) {
                        Some(r) => Pos2::new(r.origin + r.egui_x(r.pointer(p.x - r.origin)), p.y),
                        None => p,
                    }
                }
                Mapping::Flipped => Pos2::new(flip_x(span, p.x), p.y),
                Mapping::Moved(d) => Pos2::new(p.x - d, p.y),
            }
        };
        // The character the pointer puts the field's caret at.
        let field_column = |m: Mapping, p: Pos2| -> Option<usize> {
            let Mapping::Field(d) = m else { return None };
            let r = rows.iter().find(|r| (r.top..r.bottom).contains(&p.y))?;
            Some(r.start + r.pointer(p.x - d - r.origin))
        };
        let over_page = |p: Option<Pos2>| p.is_some_and(|p| !on_floating(p, None) && canvas.is_some_and(|k| shown(k, p)));
        let flip_wheel = !over_page(self.pointer);
        for event in &mut input.events {
            match event {
                Event::PointerMoved(p) => {
                    self.pointer = Some(*p);
                    let m = self.drag.unwrap_or_else(|| mapping_at(*p));
                    if let (Some(press), Some(c)) = (&mut self.field_press, field_column(m, *p)) {
                        press.1 = c;
                    }
                    *p = apply(m, *p);
                }
                // A press fixes the mapping until its release, so a drag that leaves what it started
                // on (a dialog dragged quickly over the page) doesn't jump to another mapping.
                Event::PointerButton { pos, pressed, button, modifiers } => {
                    let m = self.drag.unwrap_or_else(|| mapping_at(*pos));
                    self.drag = pressed.then_some(m);
                    if *button == egui::PointerButton::Primary {
                        let c = field_column(m, *pos);
                        match (&mut self.field_press, c) {
                            (Some(press), Some(c)) if !*pressed => press.1 = c,
                            _ if *pressed => self.field_press = c.map(|c| ((!modifiers.shift).then_some(c), c)),
                            _ => {}
                        }
                    }
                    *pos = apply(m, *pos);
                }
                Event::Touch { pos, .. } => *pos = apply(mapping_at(*pos), *pos),
                Event::PointerGone => self.pointer = None,
                Event::MouseMoved(delta) => delta.x = -delta.x,
                Event::MouseWheel { delta, .. } if flip_wheel => delta.x = -delta.x,
                _ => {}
            }
        }
    }

    fn on_end_pass(&mut self, ui: &mut Ui) {
        let ctx = ui.ctx().clone();
        let screen = ctx.content_rect();
        let span = (screen.min.x, screen.max.x);
        self.span = Some(span);
        self.anchored_rects.clear();
        self.floating.clear();
        self.floating_shapes = 0;
        self.field = None;
        self.field_id = None;
        self.field_cursor = None;
        if !self.on {
            return;
        }
        if let Some((id, rect)) = editing_field(&ctx) {
            (self.field_id, self.field) = (Some(id), Some(rect));
            self.field_cursor = self.place_field_cursor(&ctx, id);
        }
        if !ctx.input(|i| i.pointer.primary_down()) {
            self.field_press = None;
        }
        let above: Vec<LayerId> = ctx.memory(|m| m.layer_ids().filter(|l| l.order != Order::Background).collect());
        self.floating_shapes = ctx.graphics(|g| above.iter().filter_map(|l| g.get(*l)).map(|list| list.next_idx().0).sum());
        // Menus, popups and windows (the page is in the background, in the central panel).
        let anchored = &self.anchored;
        let layers: Vec<LayerId> = ctx.memory(|m| {
            let areas = m.areas();
            m.layer_ids()
                .filter(|l| matches!(l.order, Order::Middle | Order::Foreground) && areas.is_visible(l) && !anchored.contains(&l.id))
                .collect()
        });
        self.floating =
            layers.into_iter().filter_map(|l| egui::containers::AreaState::load(&ctx, l.id).filter(|s| s.interactable).map(|s| s.rect())).collect();
        let Some(canvas) = self.canvas else { return };
        // Tooltips follow the pointer: over the page they belong to the page.
        const ORDERS: [Order; 5] = [Order::Background, Order::Middle, Order::Foreground, Order::Tooltip, Order::Debug];
        let mut layers: Vec<LayerId> = self.anchored.iter().flat_map(|id| ORDERS.map(|o| LayerId::new(o, *id))).collect();
        if ctx.pointer_hover_pos().is_some_and(|p| canvas.contains(p)) {
            layers.extend(ctx.memory(|m| m.layer_ids().filter(|l| l.order == Order::Tooltip).collect::<Vec<_>>()));
        }
        // A popup opens rightwards from logical point `a` (its left edge), which is drawn at
        // `a + d` with the page. Moved here and flipped with everything else, it is mirrored about
        // that point: `x' = 2a + d - x`, so it opens leftwards from it.
        let d = keep_shift(span, canvas);
        let mut placed = Vec::new();
        for layer in layers {
            let Some(area) = ctx.memory(|m| m.area_rect(layer.id)).filter(|r| r.is_positive()) else { continue };
            let sum = 2.0 * area.min.x + d;
            let shift = Vec2::new(span.0 + span.1 - sum, 0.0);
            let found = ctx.graphics_mut(|g| {
                let Some(list) = g.get_mut(layer) else { return false };
                for i in 0..list.next_idx().0 {
                    list.mutate_shape(ShapeIdx(i), |c| {
                        c.clip_rect = c.clip_rect.translate(shift);
                        c.shape.translate(shift);
                    });
                }
                true
            });
            if found {
                placed.push((area, sum));
            }
        }
        self.anchored_rects = placed;
    }

    fn output_hook(&mut self, _ctx: &Context, output: &mut FullOutput) {
        let Some(span) = self.span.filter(|_| self.on) else { return };
        let first_floating = output.shapes.len().saturating_sub(self.floating_shapes);
        self.field_rows.clear();
        let field = self.field.map(|f| (f, self.field_cursor));
        for (i, clipped) in output.shapes.iter_mut().enumerate() {
            let canvas = self.canvas.filter(|_| i < first_floating);
            mirror_clipped(span, canvas, field, &mut self.field_rows, clipped);
        }
        if let Some(ime) = &mut output.platform_output.ime {
            let keep: Vec<Rect> = self.field.into_iter().chain(self.canvas).collect();
            ime.rect = place_rect(span, &keep, ime.rect);
            ime.cursor_rect = place_rect(span, &keep, ime.cursor_rect);
        }
        // Screen readers, magnifiers and their hit testing find each element where it is drawn.
        if let Some(update) = &mut output.platform_output.accesskit_update {
            for (_, node) in &mut update.nodes {
                if let Some(b) = node.bounds() {
                    let r = self.place(span, Rect::from_min_max(Pos2::new(b.x0 as f32, b.y0 as f32), Pos2::new(b.x1 as f32, b.y1 as f32)));
                    node.set_bounds(egui::accesskit::Rect { x0: r.min.x.into(), x1: r.max.x.into(), ..b });
                }
            }
        }
    }
}

impl Mirror {
    /// Where an element of the frame (`r`, logical) is drawn, as the pointer finds it: a popup
    /// placed from the page is mirrored about its axis, the field being edited and the page move,
    /// menus, popups and dialogs (over the page too) flip with the rest of the interface.
    fn place(&self, span: (f32, f32), r: Rect) -> Rect {
        if let Some((_, sum)) = self.anchored_rects.iter().find(|(a, _)| a.expand(1.0).contains_rect(r)) {
            return flip_rect((0.0, *sum), r);
        }
        let moved = |k: Rect| r.translate(Vec2::new(keep_shift(span, k), 0.0));
        if let Some(f) = self.field.filter(|f| f.expand(1.0).contains_rect(r)) {
            return moved(f);
        }
        match self.canvas.filter(|c| c.expand(1.0).contains_rect(r) && !self.floating.iter().any(|f| f.contains(r.center()))) {
            Some(c) => moved(c),
            None => flip_rect(span, r),
        }
    }

    /// The cursor of the field being edited, in characters. A press on it puts the caret by the
    /// letter drawn under the pointer, a drag selects from where it was pressed (egui's own
    /// placement is kept for double and triple clicks, which select a word or the line).
    fn place_field_cursor(&self, ctx: &Context, id: Id) -> Option<(usize, usize)> {
        let mut state = egui::text_edit::TextEditState::load(ctx, id)?;
        let multi = ctx.input(|i| {
            let p = &i.pointer;
            p.button_double_clicked(egui::PointerButton::Primary) || p.button_triple_clicked(egui::PointerButton::Primary)
        });
        if let Some((from, at)) = self.field_press.filter(|_| !multi) {
            let from = from.or_else(|| state.cursor.char_range().map(|r| r.secondary.index.0)).unwrap_or(at);
            let range = egui::text::CCursorRange::two(egui::text::CCursor::new(from), egui::text::CCursor::new(at));
            if state.cursor.char_range() != Some(range) {
                state.cursor.set_char_range(Some(range));
                state.clone().store(ctx, id);
            }
        }
        state.cursor.char_range().map(|r| (r.primary.index.0, r.secondary.index.0))
    }
}

/// Whether this frame is mirrored: icon letters (`x²`) are placed so the mirror keeps their order.
pub fn active(ctx: &Context) -> bool {
    ctx.plugin_opt::<Mirror>().is_some_and(|h| h.lock().on)
}

/// Turn the mirror on (a right-to-left interface) or off. `canvas` is the document canvas with
/// its rulers when it is on screen, `anchored` the area ids of popups placed from it. Call once per
/// frame, after the interface is laid out.
pub fn configure(ctx: &Context, on: bool, canvas: Option<Rect>, anchored: &[Id]) {
    if ctx.plugin_opt::<Mirror>().is_none() {
        ctx.add_plugin(Mirror::default());
    }
    if let Some(handle) = ctx.plugin_opt::<Mirror>() {
        let mut m = handle.lock();
        m.on = on;
        m.canvas = canvas;
        m.anchored = anchored.to_vec();
    }
}

/// Where the mirror draws `r`, laid out on the page (or the canvas round it): the control
/// channel's coordinates are the window's as drawn.
pub fn page_to_window(ctx: &Context, r: Rect) -> Rect {
    let Some(handle) = ctx.plugin_opt::<Mirror>() else { return r };
    let m = handle.lock();
    match (m.on, m.span, m.canvas) {
        (true, Some(span), Some(canvas)) => r.translate(Vec2::new(keep_shift(span, canvas), 0.0)),
        _ => r,
    }
}

fn flip_x((min, max): (f32, f32), x: f32) -> f32 {
    min + max - x
}

fn flip_rect(span: (f32, f32), r: Rect) -> Rect {
    if !r.is_finite() {
        return r;
    }
    Rect::from_x_y_ranges(flip_x(span, r.max.x)..=flip_x(span, r.min.x), r.y_range())
}

/// How far an area kept unflipped moves to sit where the mirror puts it.
fn keep_shift(span: (f32, f32), k: Rect) -> f32 {
    flip_x(span, k.max.x) - k.min.x
}

/// A rectangle (the input method's) placed like the shapes around it.
fn place_rect(span: (f32, f32), keep: &[Rect], r: Rect) -> Rect {
    match keep.iter().find(|k| k.expand(1.0).contains_rect(r)) {
        Some(k) => r.translate(Vec2::new(keep_shift(span, *k), 0.0)),
        None => flip_rect(span, r),
    }
}

/// The text field being edited, if any: its caret and selection are drawn apart from its text.
fn editing_field(ctx: &Context) -> Option<(Id, Rect)> {
    let id: Id = ctx.memory(|m| m.focused())?;
    egui::TextEdit::load_state(ctx, id)?;
    ctx.read_response(id).map(|r| (id, r.rect))
}

/// Mirror one shape of the finished frame: shapes clipped to the canvas, and shapes inside the
/// field being edited, move without flipping. The field's text is put in visual order, its caret
/// and selection with it (`rows`, where its glyphs went; the field comes with its cursor).
fn mirror_clipped(
    span: (f32, f32),
    canvas: Option<Rect>,
    field: Option<(Rect, Option<(usize, usize)>)>,
    rows: &mut Vec<RowMap>,
    clipped: &mut ClippedShape,
) {
    let in_canvas = canvas.filter(|c| c.expand(1.0).contains_rect(clipped.clip_rect));
    let in_field = || {
        let b = clipped.shape.visual_bounding_rect();
        field.filter(|(f, _)| b.is_positive() && f.expand(1.0).contains_rect(b))
    };
    let k = if let Some(k) = in_canvas {
        k
    } else if let Some((k, cursor)) = in_field() {
        field_bidi(&mut clipped.shape, rows, k, cursor);
        // egui clips the field's text and caret to where it laid them out; lined up on the right
        // they need the field's whole width.
        let c = &mut clipped.clip_rect;
        (c.min.x, c.max.x) = (c.min.x.min(k.min.x), c.max.x.max(k.max.x));
        k
    } else {
        clipped.clip_rect = flip_rect(span, clipped.clip_rect);
        mirror_shape(span, &mut clipped.shape);
        return;
    };
    let d = Vec2::new(keep_shift(span, k), 0.0);
    clipped.clip_rect = clipped.clip_rect.translate(d);
    clipped.shape.translate(d);
}

/// The text field being edited, as in a right-to-left interface: its text in visual order and
/// lined up on the right (mirroring its left margin), then its caret (a vertical line, drawn
/// after the text) where the letter it stands by is drawn. `cursor` (in characters: the caret and
/// the selection's other end) places the caret and the selection: egui's own columns are off in a
/// right-to-left word ([`drop_phantoms`]). Text longer than the field scrolls as egui lays it out.
fn field_bidi(shape: &mut Shape, rows: &mut Vec<RowMap>, field: Rect, cursor: Option<(usize, usize)>) {
    match shape {
        Shape::Vec(shapes) => shapes.iter_mut().for_each(|s| field_bidi(s, rows, field, cursor)),
        Shape::Text(t) => {
            let selection = cursor.map(|(a, b)| a.min(b)..a.max(b));
            let mut maps = bidi_galley(&mut t.galley, t.pos, true, selection);
            let margin = t.pos.x + t.galley.rect.left() - field.min.x;
            if (0.0..field.width() / 2.0).contains(&margin) {
                let right = field.max.x - margin;
                let galley = Arc::make_mut(&mut t.galley);
                let mut widest = 0.0f32;
                for (placed, map) in galley.rows.iter_mut().zip(maps.iter_mut()) {
                    let dx = right - (map.origin + map.end);
                    // Never further than the field is wide (a row laid out off its start scrolls).
                    if dx > 0.0 && dx < field.width() {
                        widest = widest.max(dx);
                        let row = Arc::make_mut(&mut placed.row);
                        row.visuals.mesh.vertices.iter_mut().for_each(|v| v.pos.x += dx);
                        row.visuals.mesh_bounds = row.visuals.mesh_bounds.translate(Vec2::new(dx, 0.0));
                        row.glyphs.iter_mut().for_each(|g| g.pos.x += dx);
                        map.new.iter_mut().for_each(|x| *x += dx);
                        map.shift = dx;
                    }
                }
                galley.mesh_bounds = galley.mesh_bounds.union(galley.mesh_bounds.translate(Vec2::new(widest, 0.0)));
            }
            rows.extend(maps);
        }
        Shape::LineSegment { points, .. } if (points[0].x - points[1].x).abs() < 0.01 => {
            let mid = (points[0].y + points[1].y) / 2.0;
            // An empty field draws no text: its caret goes to the right, mirroring the margin.
            if rows.is_empty() {
                let x = field.min.x + field.max.x - points[0].x;
                points[0].x = x;
                points[1].x = x;
                return;
            }
            let Some(row) = rows.iter().find(|r| (r.top..r.bottom).contains(&mid)) else { return };
            let column = match cursor {
                Some((at, _)) => Some(at.saturating_sub(row.start).min(row.shows.len())),
                None => row.column_at(points[0].x - row.origin),
            };
            let Some(column) = column else { return };
            let x = row.origin + row.caret(column);
            points[0].x = x;
            points[1].x = x;
        }
        _ => {}
    }
}

fn mirror_shape(span: (f32, f32), shape: &mut Shape) {
    let fx = |p: &mut Pos2| p.x = flip_x(span, p.x);
    match shape {
        Shape::Noop => {}
        Shape::Vec(shapes) => shapes.iter_mut().for_each(|s| mirror_shape(span, s)),
        Shape::Circle(c) => fx(&mut c.center),
        Shape::Ellipse(e) => fx(&mut e.center),
        Shape::LineSegment { points, .. } => points.iter_mut().for_each(fx),
        // A check box's tick keeps its direction, as in right-to-left Windows: it only moves.
        Shape::Path(p) if !p.closed && is_check_mark(&p.points) => {
            let r = Rect::from_points(&p.points);
            let d = flip_x(span, r.max.x) - r.min.x;
            p.points.iter_mut().for_each(|q| q.x += d);
        }
        Shape::Path(p) => p.points.iter_mut().for_each(fx),
        Shape::QuadraticBezier(b) => b.points.iter_mut().for_each(fx),
        Shape::CubicBezier(b) => b.points.iter_mut().for_each(fx),
        Shape::Rect(r) => {
            r.rect = flip_rect(span, r.rect);
            let c = &mut r.corner_radius;
            std::mem::swap(&mut c.nw, &mut c.ne);
            std::mem::swap(&mut c.sw, &mut c.se);
        }
        // Text moves as a block and keeps its glyphs, in visual order; wrapped lines align right.
        // (The page's own text, kept unflipped, never comes here: comment balloons order theirs.)
        Shape::Text(t) => {
            bidi_galley(&mut t.galley, t.pos, false, None);
            let r = t.galley.rect.translate(t.pos.to_vec2());
            t.pos.x += flip_x(span, r.max.x) - r.min.x;
            if t.galley.rows.len() > 1 {
                let galley = Arc::make_mut(&mut t.galley);
                let right = galley.rect.max.x;
                for row in &mut galley.rows {
                    row.pos.x += right - row.rect().max.x;
                }
            }
        }
        Shape::Mesh(mesh) => {
            let m = Arc::make_mut(mesh);
            if m.texture_id == egui::TextureId::default() {
                // Drawn in code (icons, arrows): flip.
                m.vertices.iter_mut().for_each(|v| fx(&mut v.pos));
            } else {
                // A picture: move it, keep it the right way round.
                let r = m.calc_bounds();
                m.translate(Vec2::new(flip_x(span, r.max.x) - r.min.x, 0.0));
            }
        }
        Shape::Callback(c) => c.rect = flip_rect(span, c.rect),
    }
}

/// A strong right-to-left letter (Hebrew, Arabic …).
fn strong_rtl(c: char) -> bool {
    matches!(bidi_class(c), BidiClass::R | BidiClass::AL)
}

/// Put a laid-out text in visual order. egui shapes each run of one font face right to left
/// when it holds right-to-left letters (a Hebrew word: its letters come from a borrowed face, the
/// spaces from the interface face), but places the runs left to right in text order, so
/// `מסך הבית` drew as `הבית מסך`. Each row is reordered with the Unicode bidirectional algorithm
/// (UAX #9), in the direction of the text's first strong letter, by moving its glyphs' vertices.
/// Comment balloons put their own text in display order before egui lays it out
/// (`canvas::balloon_text`); this does the same for any interface text, after layout. `pos` is
/// where the text is drawn; the result says where each row's glyphs went, for the text field
/// being edited (its caret, selection and pointer): with `every_row`, one map per row, also for
/// rows without right-to-left letters and empty ones. `selection` (in characters) is the field's.
fn bidi_galley(galley: &mut Arc<Galley>, pos: Pos2, every_row: bool, selection: Option<Range<usize>>) -> Vec<RowMap> {
    let text = &galley.job.text;
    if !every_row && !crate::canvas::has_rtl(text) {
        return Vec::new();
    }
    let base = text.chars().find_map(|c| match bidi_class(c) {
        BidiClass::R | BidiClass::AL => Some(Level::rtl()),
        BidiClass::L => Some(Level::ltr()),
        _ => None,
    });
    let base = base.unwrap_or_else(Level::ltr);
    let mut maps = Vec::new();
    let mut start = 0;
    // A cached galley is shared with egui's layout cache: this changes the frame's copy only.
    for placed in &mut Arc::make_mut(galley).rows {
        let at = pos + placed.pos.to_vec2();
        let selected = selection.clone().map(|s| s.start.saturating_sub(start)..s.end.saturating_sub(start));
        let map = if placed.row.glyphs.is_empty() { Some(RowMap::default()) } else { bidi_row(Arc::make_mut(&mut placed.row), base, selected) };
        if let Some(mut map) = map.or_else(|| every_row.then(RowMap::default)) {
            map.top = at.y;
            map.bottom = at.y + placed.row.size.y;
            map.origin = at.x;
            map.start = start;
            map.end = placed.row.size.x;
            maps.push(map);
        }
        // One glyph per letter now (`drop_phantoms`), and the newline ending the row.
        start += placed.row.glyphs.len() + usize::from(placed.ends_with_newline);
    }
    maps
}

/// Where the glyphs of one row went ([`bidi_row`]), for the caret, the selection and the pointer of
/// the text field being edited. egui places all three by cursor column (character): column `i` is
/// the left edge of glyph `i` as laid out.
#[derive(Clone, Debug, Default, PartialEq)]
struct RowMap {
    /// The row's top and bottom, and where its x coordinates start (absolute, as laid out).
    top: f32,
    bottom: f32,
    origin: f32,
    /// The row's first character in the text.
    start: usize,
    /// Where the row's text ends, as laid out.
    end: f32,
    /// How far the row moved to line up on the right (the field of a right-to-left interface).
    shift: f32,
    /// Per column, up to the row's length: where egui laid out the caret there.
    old: Vec<f32>,
    /// The middles of the glyphs egui finds a column by (the pointer left of glyph `i`'s middle,
    /// and right of the ones before, is at column `i`), phantoms and all ([`drop_phantoms`]).
    middles: Vec<f32>,
    /// Per glyph (one per letter): its width, its left edge as drawn, and whether it is drawn right
    /// to left.
    width: Vec<f32>,
    new: Vec<f32>,
    rtl: Vec<bool>,
    /// The glyph that draws column `i`'s letter (and the column glyph `i` draws: the map is its own
    /// inverse). A shaped right-to-left word holds its glyphs in drawing order, last letter first.
    shows: Vec<usize>,
}

impl RowMap {
    /// The caret before `column` (after the row's last letter when it is the row's length), as
    /// drawn, in row coordinates: before a right-to-left letter is its right edge.
    fn caret(&self, column: usize) -> f32 {
        let n = self.shows.len();
        if column < n {
            let g = self.shows[column];
            if self.rtl[g] { self.new[g] + self.width[g] } else { self.new[g] }
        } else if let Some(&g) = n.checked_sub(1).and_then(|last| self.shows.get(last)) {
            if self.rtl[g] { self.new[g] } else { self.new[g] + self.width[g] }
        } else {
            self.shift
        }
    }

    /// The column whose laid-out x is `x` (row coordinates), or the row's length at its end; an empty
    /// row's caret is at its start.
    fn column_at(&self, x: f32) -> Option<usize> {
        let near = |a: f32| (a - x).abs() < 1.0;
        if self.old.is_empty() {
            return near(self.end).then_some(0);
        }
        (0..self.old.len()).filter(|&i| near(self.old[i])).min_by(|&a, &b| (self.old[a] - x).abs().total_cmp(&(self.old[b] - x).abs()))
    }

    /// For a pointer drawn at `x` (row coordinates), the column the caret goes to: before or after
    /// the letter under the pointer, by which half it is on.
    fn pointer(&self, x: f32) -> usize {
        let n = self.shows.len();
        let off = |g: usize| if x < self.new[g] { self.new[g] - x } else { (x - self.new[g] - self.width[g]).max(0.0) };
        let Some(g) = (0..n).min_by(|&a, &b| off(a).total_cmp(&off(b))) else { return 0 };
        let mid = self.new[g] + self.width[g] / 2.0;
        let before = if self.rtl[g] { x >= mid } else { x < mid };
        self.shows[g] + usize::from(!before)
    }

    /// An x (row coordinates, as laid out) at which egui finds `column`: past the middles of the
    /// glyphs before it, short of its own. Where phantoms leave none, the press is put right after
    /// egui handles it ([`Mirror::place_field_cursor`]).
    fn egui_x(&self, column: usize) -> f32 {
        let before = self.middles[..column.min(self.middles.len())].iter().copied().reduce(f32::max);
        match (before, self.middles.get(column)) {
            (None, Some(&at)) => at - 1.0,
            (Some(before), Some(&at)) => (before + at) / 2.0,
            (Some(before), None) => before + 1.0,
            (None, None) => 0.0,
        }
    }
}

/// One row of [`bidi_galley`]; `selection` (in characters, from the row's start) is the field's.
fn bidi_row(row: &mut Row, base: Level, selection: Option<Range<usize>>) -> Option<RowMap> {
    // egui finds and draws its columns by glyph, phantoms and all: column `i` at glyph `i`, coloured
    // as selected or not by egui.
    let laid: Vec<f32> = row.glyphs.iter().map(|g| g.pos.x).collect();
    let middles: Vec<f32> = row.glyphs.iter().map(|g| g.pos.x + g.advance_width / 2.0).collect();
    let column_colours = glyph_colours(row);
    drop_phantoms(row);
    let glyphs = &row.glyphs;
    let n = glyphs.len();
    // What moves as one: a run of one face holding right-to-left letters (already drawn right to
    // left by the shaper), or any other single glyph. Faces are told apart by their metrics.
    let face = |i: usize| (glyphs[i].font_face_ascent.to_bits(), glyphs[i].font_face_height.to_bits());
    let mut units: Vec<(Range<usize>, bool)> = Vec::new();
    let mut i = 0;
    while i < n {
        let mut j = i + 1;
        while j < n && face(j) == face(i) {
            j += 1;
        }
        if glyphs[i..j].iter().any(|g| strong_rtl(g.chr)) {
            units.push((i..j, true));
        } else {
            units.extend((i..j).map(|k| (k..k + 1, false)));
        }
        i = j;
    }
    // The row's text in logical order (a shaped right-to-left run's glyphs come last letter first).
    let mut text = String::new();
    let mut starts = Vec::with_capacity(units.len());
    for (r, rtl) in &units {
        starts.push(text.len());
        if *rtl {
            text.extend(glyphs[r.clone()].iter().rev().map(|g| g.chr));
        } else {
            text.extend(glyphs[r.clone()].iter().map(|g| g.chr));
        }
    }
    let info = BidiInfo::new(&text, Some(base));
    let [para] = info.paragraphs.as_slice() else { return None };
    let levels = info.reordered_levels(para, para.range.clone());
    let unit_levels: Vec<Level> = starts.iter().map(|&s| levels.get(s).copied().unwrap_or(base)).collect();
    let order = BidiInfo::reorder_visual(&unit_levels);
    // Each unit's left edge and width as laid out, then its left edge in visual order.
    let left: Vec<f32> = units.iter().map(|(r, _)| glyphs[r.start].pos.x).collect();
    let end = glyphs[n - 1].max_x();
    let mut shift = vec![0.0f32; units.len()];
    let mut x = left[0];
    for &k in &order {
        shift[k] = x - left[k];
        x += left.get(k + 1).copied().unwrap_or(end) - left[k];
    }
    let old = (0..=n).map(|c| laid.get(c).copied().unwrap_or(row.size.x)).collect();
    let mut map = RowMap { end, old, middles, ..RowMap::default() };
    for (k, (r, rtl_run)) in units.iter().enumerate() {
        for gi in r.clone() {
            map.width.push(glyphs[gi].advance_width);
            map.new.push(glyphs[gi].pos.x + shift[k]);
            map.rtl.push(unit_levels[k].is_rtl());
            map.shows.push(if *rtl_run { r.start + r.end - 1 - gi } else { gi });
        }
    }
    let glyph_end = row.visuals.glyph_vertex_range.end;
    let ranges = glyph_ranges(row);
    let mut moves: Vec<(usize, f32, bool)> = Vec::new();
    for (k, (r, _)) in units.iter().enumerate() {
        for gi in r.clone() {
            // Paired punctuation inside right-to-left text faces the other way (as in the
            // balloons): `(עמוד)` keeps its parentheses round the word.
            let c = glyphs[gi].chr;
            let flip = unit_levels[k].is_rtl() && r.len() == 1 && crate::canvas::mirrored(c) != c;
            if shift[k] != 0.0 || flip {
                moves.push((gi, shift[k], flip));
            }
        }
    }
    let quads = selection_quads(row, glyph_end);
    if !quads.is_empty() {
        recolour_selected_letters(row, &map, &ranges, selection.as_ref().map(|s| (s, column_colours.as_slice())));
    }
    let vertices = &mut row.visuals.mesh.vertices;
    for (gi, dx, flip) in moves {
        row.glyphs[gi].pos.x += dx;
        let Some(vs) = vertices.get_mut(ranges[gi].clone()) else { continue };
        if flip {
            let (lo, hi) = vs.iter().fold((f32::MAX, f32::MIN), |(lo, hi), v| (lo.min(v.pos.x), hi.max(v.pos.x)));
            vs.iter_mut().for_each(|v| v.pos.x = lo + hi - v.pos.x);
        }
        vs.iter_mut().for_each(|v| v.pos.x += dx);
    }
    for first in quads {
        place_selection(row, &map, first, selection.as_ref());
    }
    Some(map)
}

/// The vertices of each glyph of a row: up to the next glyph's first one.
fn glyph_ranges(row: &Row) -> Vec<Range<usize>> {
    let glyphs = &row.glyphs;
    let end = row.visuals.glyph_vertex_range.end;
    (0..glyphs.len())
        .map(|i| {
            let first = glyphs[i].first_vertex as usize;
            first..glyphs.get(i + 1).map_or(end, |g| (g.first_vertex as usize).max(first))
        })
        .collect()
}

/// The colour each glyph of a row is drawn in (none for a glyph that draws nothing).
fn glyph_colours(row: &Row) -> Vec<Option<egui::Color32>> {
    let vs = &row.visuals.mesh.vertices;
    glyph_ranges(row).into_iter().map(|r| vs.get(r.start).filter(|_| !r.is_empty()).map(|v| v.color)).collect()
}

/// epaint 0.36 pads a shaped right-to-left word with a zero-width glyph that draws nothing for each
/// letter after its first: it takes the cluster of the word's last glyph (its first letter, drawn
/// at the left) to run to the word's end. The row then holds more glyphs than letters, and egui,
/// which places a column by glyph (column `i` at glyph `i`), puts its caret and selection short of
/// where they belong. They go here, leaving one glyph per letter.
fn drop_phantoms(row: &mut Row) {
    let glyphs = &row.glyphs;
    let face = |i: usize| (glyphs[i].font_face_ascent.to_bits(), glyphs[i].font_face_height.to_bits());
    let phantom = |i: usize| glyphs[i].advance_width == 0.0 && glyphs[i].uv_rect.is_nothing();
    let mut keep = vec![true; glyphs.len()];
    let mut i = 0;
    while i < glyphs.len() {
        let mut j = i + 1;
        while j < glyphs.len() && face(j) == face(i) {
            j += 1;
        }
        if glyphs[i..j].iter().any(|g| strong_rtl(g.chr)) {
            let mut k = j;
            while k > i + 1 && phantom(k - 1) {
                k -= 1;
                keep[k] = false;
            }
        }
        i = j;
    }
    if keep.contains(&false) {
        let mut kept = keep.into_iter();
        row.glyphs.retain(|_| kept.next().unwrap_or(true));
    }
}

/// The selection rectangles egui added to a row (four vertices each, after the glyphs, as tall as
/// the row; an underline is a thin strip): their first vertex.
fn selection_quads(row: &Row, glyph_end: usize) -> Vec<usize> {
    let vs = &row.visuals.mesh.vertices;
    let mut out = Vec::new();
    let mut i = glyph_end;
    while i + 4 <= vs.len() {
        let q = &vs[i..i + 4];
        let (top, bottom) = q.iter().fold((f32::MAX, f32::MIN), |(t, b), v| (t.min(v.pos.y), b.max(v.pos.y)));
        if q.iter().all(|v| v.color == q[0].color) && bottom - top >= row.size.y * 0.9 {
            out.push(i);
        }
        i += 4;
    }
    out
}

/// egui colours the selected letters by column, but a shaped right-to-left word draws column `i`'s
/// letter with another glyph: give each glyph the colour of the column it draws. With the field's
/// `selection`, egui's colours (`column_colours`, glyph `i`'s for column `i`, phantoms and all) give
/// the selected letters' colour and the others'.
fn recolour_selected_letters(row: &mut Row, map: &RowMap, ranges: &[Range<usize>], selection: Option<(&Range<usize>, &[Option<egui::Color32>])>) {
    let vs = &mut row.visuals.mesh.vertices;
    let colour: Vec<Option<egui::Color32>> = match selection {
        Some((selected, column_colours)) => {
            let first = |inside: bool| column_colours.iter().enumerate().find_map(|(i, c)| c.filter(|_| selected.contains(&i) == inside));
            let (on, off) = (first(true), first(false));
            (0..map.shows.len()).map(|c| if selected.contains(&c) { on } else { off }).collect()
        }
        None => ranges.iter().map(|r| vs.get(r.start).filter(|_| !r.is_empty()).map(|v| v.color)).collect(),
    };
    for (g, r) in ranges.iter().enumerate() {
        if let (Some(c), Some(vs)) = (colour[map.shows[g]], vs.get_mut(r.clone())) {
            vs.iter_mut().for_each(|v| v.color = c);
        }
    }
}

/// Put a selection rectangle (egui's, laid out from one column to another) under the selected
/// letters as drawn: one rectangle for each stretch of them, which in mixed text needn't be one.
/// The field's `selection` (in characters, from the row's start) says which they are where known.
fn place_selection(row: &mut Row, map: &RowMap, first: usize, selection: Option<&Range<usize>>) {
    let vs = &row.visuals.mesh.vertices;
    let (left, right) = vs[first..first + 4].iter().fold((f32::MAX, f32::MIN), |(l, r), v| (l.min(v.pos.x), r.max(v.pos.x)));
    let n = map.shows.len();
    let (from, to) = match selection {
        Some(s) => (s.start.min(n), s.end.min(n)),
        None => {
            let from = if left <= map.old.first().copied().unwrap_or(0.0) + 0.5 { 0 } else { map.column_at(left).unwrap_or(0) };
            (from, map.column_at(right).unwrap_or(n).min(n))
        }
    };
    let newline = match selection {
        Some(s) => (s.end > n).then_some(4.0),
        None => (right > map.end + 0.5).then_some(right - map.end),
    };
    let mut spans: Vec<(f32, f32)> = (from..to).map(|c| map.shows[c]).map(|g| (map.new[g], map.new[g] + map.width[g])).collect();
    spans.sort_by(|a, b| a.0.total_cmp(&b.0));
    let mut merged: Vec<(f32, f32)> = Vec::new();
    for (a, b) in spans {
        match merged.last_mut() {
            Some(last) if a <= last.1 + 0.5 => last.1 = last.1.max(b),
            _ => merged.push((a, b)),
        }
    }
    // The selected newline at a row's end keeps its strip past the text.
    if let (Some(extra), Some(last)) = (newline, merged.last_mut()) {
        last.1 += extra;
    }
    let Some(&(a, b)) = merged.first() else {
        // Nothing of this row is selected after all: the rectangle goes.
        row.visuals.mesh.vertices[first..first + 4].iter_mut().for_each(|v| v.pos.x = left);
        return;
    };
    let set_x = |vs: &mut [egui::epaint::Vertex], (a, b): (f32, f32)| {
        for v in vs.iter_mut() {
            v.pos.x = if (v.pos.x - left).abs() <= (v.pos.x - right).abs() { a } else { b };
        }
    };
    let quad: Vec<egui::epaint::Vertex> = vs[first..first + 4].to_vec();
    set_x(&mut row.visuals.mesh.vertices[first..first + 4], (a, b));
    // Further stretches: copies of the rectangle, drawn behind the glyphs like egui's.
    let mesh = &mut row.visuals.mesh;
    let at = row.visuals.glyph_index_start;
    for &span in &merged[1..] {
        let base = mesh.vertices.len() as u32;
        let mut copy = quad.clone();
        set_x(&mut copy, span);
        mesh.vertices.extend(copy);
        let tri = [base, base + 1, base + 2, base + 2, base + 1, base + 3];
        mesh.indices.splice(at..at, tri);
    }
}

/// egui's check box tick: a line from the middle of a square's left side to the middle of its
/// bottom, then up to its top right corner.
fn is_check_mark(p: &[Pos2]) -> bool {
    let [a, b, c] = p else { return false };
    let close = |x: f32, y: f32| (x - y).abs() < 0.01;
    a.x < b.x && b.x < c.x && close(b.x - a.x, c.x - b.x) && c.y < a.y && a.y < b.y && close(a.y, (b.y + c.y) / 2.0) && close(c.x - a.x, b.y - c.y)
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::{Color32, Stroke, pos2};

    const SPAN: (f32, f32) = (0.0, 100.0);

    #[test]
    fn shapes_flip_about_the_window_centre() {
        let rect = Rect::from_min_max(pos2(10.0, 0.0), pos2(30.0, 5.0));
        let mut filled = ClippedShape { clip_rect: Rect::EVERYTHING, shape: Shape::rect_filled(rect, 0.0, Color32::RED) };
        mirror_clipped(SPAN, None, None, &mut Vec::new(), &mut filled);
        assert_eq!(filled.shape.visual_bounding_rect(), Rect::from_min_max(pos2(70.0, 0.0), pos2(90.0, 5.0)));
        let mut line = Shape::line_segment([pos2(0.0, 1.0), pos2(20.0, 1.0)], Stroke::new(1.0, Color32::RED));
        mirror_shape(SPAN, &mut line);
        assert!(matches!(line, Shape::LineSegment { points, .. } if points == [pos2(100.0, 1.0), pos2(80.0, 1.0)]));
    }

    #[test]
    fn the_page_and_the_field_being_edited_move_without_flipping() {
        // The canvas spans 0–60 of a 100-wide window: mirrored it spans 40–100, its contents in
        // the same order.
        let canvas = Rect::from_min_max(pos2(0.0, 0.0), pos2(60.0, 50.0));
        let caret = Rect::from_min_max(pos2(5.0, 0.0), pos2(6.0, 9.0));
        let mut on_page = ClippedShape { clip_rect: canvas, shape: Shape::rect_filled(caret, 0.0, Color32::BLACK) };
        mirror_clipped(SPAN, Some(canvas), None, &mut Vec::new(), &mut on_page);
        assert_eq!(on_page.shape.visual_bounding_rect().min.x, 45.0);
        assert_eq!(on_page.clip_rect, Rect::from_min_max(pos2(40.0, 0.0), pos2(100.0, 50.0)));
        // A field at 70–90 mirrors to 10–30; its caret at 72 lands at 12, still near its start.
        let field = Rect::from_min_max(pos2(70.0, 60.0), pos2(90.0, 70.0));
        let mut in_field = ClippedShape {
            clip_rect: Rect::EVERYTHING,
            shape: Shape::rect_filled(Rect::from_min_max(pos2(72.0, 61.0), pos2(73.0, 69.0)), 0.0, Color32::BLACK),
        };
        mirror_clipped(SPAN, Some(canvas), Some((field, None)), &mut Vec::new(), &mut in_field);
        assert_eq!(in_field.shape.visual_bounding_rect().min.x, 12.0);
        assert_eq!(place_rect(SPAN, &[canvas], caret).min.x, 45.0);
    }

    #[test]
    fn the_pointer_is_flipped_back_to_where_widgets_are_laid_out() {
        let canvas = Rect::from_min_max(pos2(0.0, 0.0), pos2(60.0, 50.0));
        let mut m = Mirror { on: true, canvas: Some(canvas), ..Default::default() };
        let mut input = RawInput {
            screen_rect: Some(Rect::from_min_max(pos2(0.0, 0.0), pos2(100.0, 80.0))),
            events: vec![
                Event::PointerMoved(pos2(90.0, 70.0)),
                Event::MouseWheel {
                    unit: egui::MouseWheelUnit::Point,
                    delta: egui::vec2(3.0, 1.0),
                    modifiers: Default::default(),
                    phase: egui::TouchPhase::Move,
                },
                Event::PointerMoved(pos2(45.0, 10.0)),
            ],
            ..Default::default()
        };
        egui::Plugin::input_hook(&mut m, &Context::default(), &mut input);
        assert!(matches!(input.events[0], Event::PointerMoved(p) if p == pos2(10.0, 70.0)), "outside the page: flipped");
        assert!(matches!(input.events[1], Event::MouseWheel { delta, .. } if delta == egui::vec2(-3.0, 1.0)), "sideways scrolling flips");
        assert!(matches!(input.events[2], Event::PointerMoved(p) if p == pos2(5.0, 10.0)), "on the mirrored page: shifted");
        // A context menu laid out at 10–30 and mirrored about 130 (`2a + d`) is drawn at 100–120.
        m.anchored_rects = vec![(Rect::from_min_max(pos2(10.0, 0.0), pos2(30.0, 40.0)), 130.0)];
        let mut on_menu = RawInput { events: vec![Event::PointerMoved(pos2(112.0, 5.0))], ..Default::default() };
        egui::Plugin::input_hook(&mut m, &Context::default(), &mut on_menu);
        assert!(matches!(on_menu.events[0], Event::PointerMoved(p) if p == pos2(18.0, 5.0)), "on a popup from the page: its own axis");
        // The Font list, laid out at 50–75 and flipped to 25–50, hangs over the page (drawn at
        // 40–100): the pointer there goes to the list, beside it to the page.
        m.floating = vec![Rect::from_min_max(pos2(50.0, 0.0), pos2(75.0, 40.0))];
        let mut list = RawInput { events: vec![Event::PointerMoved(pos2(45.0, 5.0)), Event::PointerMoved(pos2(55.0, 5.0))], ..Default::default() };
        egui::Plugin::input_hook(&mut m, &Context::default(), &mut list);
        assert!(matches!(list.events[0], Event::PointerMoved(p) if p == pos2(55.0, 5.0)), "on a menu over the page: flipped");
        assert!(matches!(list.events[1], Event::PointerMoved(p) if p == pos2(15.0, 5.0)), "beside it, on the page: shifted");
        m.on = false;
        let mut plain = RawInput { events: vec![Event::PointerMoved(pos2(90.0, 70.0))], ..Default::default() };
        egui::Plugin::input_hook(&mut m, &Context::default(), &mut plain);
        assert!(matches!(plain.events[0], Event::PointerMoved(p) if p == pos2(90.0, 70.0)), "off: untouched");
    }

    /// A dialog over the page is flipped like the rest of the interface: its shapes (the last of
    /// the frame) aren't taken for the page's, even where they're clipped inside it.
    #[test]
    fn dialogs_over_the_page_are_flipped() {
        let canvas = Rect::from_min_max(pos2(0.0, 0.0), pos2(60.0, 50.0));
        let mut m = Mirror { on: true, canvas: Some(canvas), span: Some(SPAN), floating_shapes: 1, ..Default::default() };
        let r = Rect::from_min_max(pos2(10.0, 10.0), pos2(20.0, 20.0));
        let mut out = egui::FullOutput::default();
        for _ in 0..2 {
            out.shapes.push(ClippedShape { clip_rect: r, shape: Shape::rect_filled(r, 0.0, Color32::BLACK) });
        }
        egui::Plugin::output_hook(&mut m, &Context::default(), &mut out);
        assert_eq!(out.shapes[0].shape.visual_bounding_rect().min.x, 50.0, "the page's: moved with the page");
        assert_eq!(out.shapes[1].shape.visual_bounding_rect().min.x, 80.0, "the dialog's: flipped");
        // Its text box being edited is drawn moved, so the pointer goes to it the same way.
        m.field = Some(Rect::from_min_max(pos2(30.0, 0.0), pos2(50.0, 10.0)));
        m.floating = vec![Rect::from_min_max(pos2(25.0, 0.0), pos2(55.0, 40.0))];
        let mut input = RawInput { events: vec![Event::PointerMoved(pos2(52.0, 5.0)), Event::PointerMoved(pos2(52.0, 20.0))], ..Default::default() };
        egui::Plugin::input_hook(&mut m, &Context::default(), &mut input);
        assert!(matches!(input.events[0], Event::PointerMoved(p) if p == pos2(32.0, 5.0)), "in the field: moved");
        assert!(matches!(input.events[1], Event::PointerMoved(p) if p == pos2(48.0, 20.0)), "elsewhere in the dialog: flipped");
    }

    /// Catalog text stays in logical order; painted, it reads right to left: the first word at the
    /// right, numbers left to right in their place. Laid out by egui with the face the Hebrew
    /// interface borrows (skipped where no installed font has Hebrew letters).
    #[test]
    fn right_to_left_labels_are_painted_in_visual_order() {
        let ctx = Context::default();
        let mut app = crate::WordApp::new(wordcraft_engine::Session::new(wordcraft_doc::Document::new()), Default::default());
        app.run("ui.language", serde_json::json!({"value": "he"})).unwrap();
        for _ in 0..2 {
            ctx.run_ui(RawInput::default(), |ui| app.logic(ui.ctx())).drop_without_applying_deltas();
        }
        if !app.canvas.ime_faces.iter().any(|f| f.covers('ק')) {
            eprintln!("no installed font has Hebrew letters: skipped");
            return;
        }
        let font = egui::FontId::proportional(14.0);
        let painted = |text: &str| {
            let mut galley = ctx.fonts_mut(|f| f.layout_no_wrap(text.to_string(), font.clone(), Color32::BLACK));
            bidi_galley(&mut galley, Pos2::ZERO, false, None);
            let row = &galley.rows[0].row;
            let x = |c: char| row.glyphs.iter().find(|g| g.chr == c).map(|g| g.pos.x).unwrap_or_else(|| panic!("{c} in {text}"));
            // Every glyph's vertices moved with it.
            for (i, g) in row.glyphs.iter().enumerate() {
                let end = row.glyphs.get(i + 1).map_or(row.visuals.glyph_vertex_range.end, |n| n.first_vertex as usize);
                for v in &row.visuals.mesh.vertices[g.first_vertex as usize..end] {
                    assert!(v.pos.x >= g.pos.x - 2.0 && v.pos.x <= g.max_x() + 2.0, "{} in {text}", g.chr);
                }
            }
            [x('מ'), x('ך'), x('ה'), x('ת')]
        };
        // מסך הבית (Home Screen): read right to left, מ at the right end, ת at the left.
        let [mem, khaf, he, tav] = painted("מסך הבית");
        assert!(mem > khaf && khaf > he && he > tav, "{mem} {khaf} {he} {tav}");
        let mut galley = ctx.fonts_mut(|f| f.layout_no_wrap("עמוד 3 מתוך 9".to_string(), font.clone(), Color32::BLACK));
        bidi_galley(&mut galley, Pos2::ZERO, false, None);
        let row = &galley.rows[0].row;
        let x = |c: char| row.glyphs.iter().find(|g| g.chr == c).map(|g| g.pos.x).unwrap();
        assert!(x('9') < x('ת') && x('ת') < x('3') && x('3') < x('ע'), "page 3 of 9 reads 9 at the left, עמוד at the right");
        // Text without right-to-left letters is left alone.
        let mut latin = ctx.fonts_mut(|f| f.layout_no_wrap("Page 3 (of 9)".to_string(), font.clone(), Color32::BLACK));
        let before: Vec<f32> = latin.rows[0].row.glyphs.iter().map(|g| g.pos.x).collect();
        bidi_galley(&mut latin, Pos2::ZERO, false, None);
        assert_eq!(latin.rows[0].row.glyphs.iter().map(|g| g.pos.x).collect::<Vec<_>>(), before);
    }

    /// Hebrew typed into a field: drawn right to left against the field's right edge (its left
    /// margin mirrored), the caret after the last letter typed, at the text's left end.
    #[test]
    fn a_field_being_edited_lines_up_right_with_the_caret_after_the_last_letter() {
        let ctx = Context::default();
        let mut app = crate::WordApp::new(wordcraft_engine::Session::new(wordcraft_doc::Document::new()), Default::default());
        app.run("ui.language", serde_json::json!({"value": "he"})).unwrap();
        for _ in 0..2 {
            ctx.run_ui(RawInput::default(), |ui| app.logic(ui.ctx())).drop_without_applying_deltas();
        }
        if !app.canvas.ime_faces.iter().any(|f| f.covers('ק')) {
            eprintln!("no installed font has Hebrew letters: skipped");
            return;
        }
        configure(&ctx, true, None, &[]);
        let id = egui::Id::new("hebrew field");
        let mut text = String::from("אני רק בודק");
        let screen = RawInput { screen_rect: Some(Rect::from_min_size(Pos2::ZERO, egui::vec2(600.0, 300.0))), ..Default::default() };
        let mut placed = None;
        let mut frame = |ctx: &Context, text: &mut String| {
            let mut out = ctx.run_ui(screen.clone(), |ui| {
                let out = egui::TextEdit::singleline(text).id(id).desired_width(300.0).show(ui);
                out.response.request_focus();
                placed = Some((out.response.rect, out.galley_pos));
            });
            out.textures_delta.clear();
            out
        };
        frame(&ctx, &mut text);
        let mut state = egui::text_edit::TextEditState::load(&ctx, id).unwrap_or_default();
        let end = egui::text::CCursor::new(text.chars().count());
        state.cursor.set_char_range(Some(egui::text::CCursorRange::one(end)));
        state.store(&ctx, id);
        frame(&ctx, &mut text);
        let out = frame(&ctx, &mut text);
        let (field, galley_pos) = placed.expect("the field was shown");
        // The field moves to its mirrored place without flipping.
        let d = keep_shift((0.0, 600.0), field);
        let margin = galley_pos.x - field.min.x;
        let mut letters = (f32::MAX, f32::MIN);
        let mut caret = None;
        let mut walk = Vec::new();
        for c in &out.shapes {
            walk.push(c.shape.clone());
        }
        while let Some(shape) = walk.pop() {
            match shape {
                Shape::Vec(v) => walk.extend(v),
                Shape::Text(t) if t.galley.job.text.contains('ק') => {
                    for placed in &t.galley.rows {
                        for g in &placed.row.glyphs {
                            let x = t.pos.x + placed.pos.x + g.pos.x;
                            letters = (letters.0.min(x), letters.1.max(x + g.advance_width));
                        }
                    }
                }
                Shape::LineSegment { points, .. } if (points[0].x - points[1].x).abs() < 0.01 => caret = Some(points[0].x),
                _ => {}
            }
        }
        let caret = caret.expect("the caret is painted");
        assert!((letters.1 - (field.max.x + d - margin)).abs() < 1.5, "lined up on the right: {letters:?}, field {field:?} + {d}");
        assert!((caret - letters.0).abs() < 1.5, "caret {caret} after the last letter, at the left end {}", letters.0);
    }

    /// A dialog dragged quickly: the pointer leaves the dialog for the page between two frames, but
    /// keeps the mapping it was pressed with until the button is released, so the dialog doesn't
    /// jump to its mirrored place.
    #[test]
    fn a_drag_keeps_the_mapping_it_started_with() {
        let canvas = Rect::from_min_max(pos2(0.0, 0.0), pos2(60.0, 50.0));
        // A dialog laid out at 50–75 is drawn at 25–50, over the page (drawn at 40–100).
        let mut m =
            Mirror { on: true, canvas: Some(canvas), floating: vec![Rect::from_min_max(pos2(50.0, 0.0), pos2(75.0, 40.0))], ..Default::default() };
        let button = |pos, pressed| Event::PointerButton { pos, button: egui::PointerButton::Primary, pressed, modifiers: Default::default() };
        let mut input = RawInput {
            screen_rect: Some(Rect::from_min_max(pos2(0.0, 0.0), pos2(100.0, 80.0))),
            events: vec![
                button(pos2(30.0, 5.0), true),
                Event::PointerMoved(pos2(70.0, 5.0)),
                button(pos2(70.0, 5.0), false),
                Event::PointerMoved(pos2(80.0, 6.0)),
            ],
            ..Default::default()
        };
        egui::Plugin::input_hook(&mut m, &Context::default(), &mut input);
        assert!(matches!(input.events[0], Event::PointerButton { pos, .. } if pos == pos2(70.0, 5.0)), "pressed on the dialog: flipped");
        assert!(matches!(input.events[1], Event::PointerMoved(p) if p == pos2(30.0, 5.0)), "dragged over the page: still flipped");
        assert!(matches!(input.events[2], Event::PointerButton { pos, .. } if pos == pos2(30.0, 5.0)), "released: flipped");
        assert!(
            matches!(input.events[3], Event::PointerMoved(p) if p == pos2(40.0, 6.0)),
            "after the release, the page's mapping (moved, not flipped to 20)"
        );
    }

    /// The caret and the pointer in a field being edited follow the letters as drawn. `אב` is one
    /// shaped run: egui holds its glyphs last letter first (ב at 0–10, א at 10–20), while its
    /// cursor columns count letters in reading order.
    #[test]
    fn the_caret_and_pointer_follow_right_to_left_letters() {
        let map = RowMap {
            end: 20.0,
            old: vec![0.0, 10.0, 20.0],
            middles: vec![5.0, 15.0],
            width: vec![10.0, 10.0],
            new: vec![0.0, 10.0],
            rtl: vec![true, true],
            shows: vec![1, 0],
            ..RowMap::default()
        };
        assert_eq!(map.caret(0), 20.0, "before א: the word's right end");
        assert_eq!(map.caret(1), 10.0, "between א and ב");
        assert_eq!(map.caret(2), 0.0, "after ב: the word's left end");
        assert_eq!(map.column_at(10.0), Some(1));
        assert_eq!(map.column_at(20.0), Some(2), "the row's end");
        // On א's right half the caret goes before it (column 0), on its left half after it.
        assert_eq!(map.pointer(18.0), 0);
        assert_eq!(map.pointer(12.0), 1);
        // On ב's left half: after ב, the end of the row.
        assert_eq!(map.pointer(3.0), 2);
        // Past the text: the nearest letter's side.
        assert_eq!(map.pointer(40.0), 0);
        // Where egui finds each column: short of its glyph's middle, past the ones before.
        assert_eq!([map.egui_x(0), map.egui_x(1), map.egui_x(2)], [4.0, 10.0, 16.0]);
    }

    /// Screen readers and the control channel find things where they are drawn: on the page they
    /// move with it, a dialog over the page flips with the interface, a popup placed from the page
    /// is mirrored about its axis, and the rest of the interface flips.
    #[test]
    fn accessibility_bounds_and_control_coordinates_follow_the_mirror() {
        let canvas = Rect::from_min_max(pos2(0.0, 0.0), pos2(60.0, 50.0));
        let m = Mirror {
            on: true,
            span: Some(SPAN),
            canvas: Some(canvas),
            floating: vec![Rect::from_min_max(pos2(50.0, 0.0), pos2(75.0, 40.0))],
            anchored_rects: vec![(Rect::from_min_max(pos2(10.0, 30.0), pos2(20.0, 45.0)), 70.0)],
            ..Default::default()
        };
        let r = |x0: f32, x1: f32, y: f32| Rect::from_min_max(pos2(x0, y), pos2(x1, y + 4.0));
        assert_eq!(m.place(SPAN, r(5.0, 9.0, 5.0)), r(45.0, 49.0, 5.0), "on the page (drawn at 40–100): moved");
        assert_eq!(m.place(SPAN, r(52.0, 56.0, 5.0)), r(44.0, 48.0, 5.0), "a dialog over the page: flipped");
        assert_eq!(m.place(SPAN, r(12.0, 14.0, 32.0)), r(56.0, 58.0, 32.0), "a popup from the page: about its axis");
        assert_eq!(m.place(SPAN, r(80.0, 90.0, 5.0)), r(10.0, 20.0, 5.0), "the interface: flipped");
        let ctx = Context::default();
        ctx.add_plugin(m);
        assert_eq!(page_to_window(&ctx, r(5.0, 9.0, 5.0)), r(45.0, 49.0, 5.0), "ui.inspect's page rectangles, as drawn");
    }

    /// A check box's tick only moves: flipped, it would point the wrong way.
    #[test]
    fn check_marks_keep_their_direction() {
        let tick = vec![pos2(10.0, 15.0), pos2(15.0, 20.0), pos2(20.0, 10.0)];
        let mut s = Shape::line(tick, Stroke::new(1.0, Color32::BLACK));
        mirror_shape(SPAN, &mut s);
        assert!(matches!(&s, Shape::Path(p) if p.points == vec![pos2(80.0, 15.0), pos2(85.0, 20.0), pos2(90.0, 10.0)]));
        let mut chevron = Shape::line(vec![pos2(10.0, 10.0), pos2(15.0, 15.0), pos2(10.0, 20.0)], Stroke::new(1.0, Color32::BLACK));
        mirror_shape(SPAN, &mut chevron);
        assert!(matches!(&chevron, Shape::Path(p) if p.points[1] == pos2(85.0, 15.0) && p.points[0] == pos2(90.0, 10.0)), "other lines flip");
    }
}
