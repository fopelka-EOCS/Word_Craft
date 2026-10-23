//! Modify Style (#2917, #2421): a style's name, its following style and its formatting, as in
//! Word: a formatting row (font, size, bold, italic, underline, colour, alignment, line spacing,
//! space before and after), a live preview, a description, and Format › Font… / Paragraph… for
//! the rest (character spacing, scale and position; indents and every line spacing rule).
//! Nothing changes until OK, which sends every change as one `styles.modify` (one undo step).

use egui::{Ui, vec2};
use serde::Serialize;
use serde_json::json;
use wordcraft_doc::props::{Align, CharProps, LineSpacing, ParaProps, Rgb, TextColor, Underline};
use wordcraft_doc::resolve::{ResolvedChar, ResolvedPara};
use wordcraft_doc::styles::{Style, StyleKind};

use crate::WordApp;
use crate::dialogs::{Dialog, buttons_with, color_field, describe};
use crate::theme::{Tokens, semibold};

/// The Modify Style dialog's state.
#[derive(Clone, Debug, Serialize)]
pub struct ModifyForm {
    /// Opened from Manage Styles, which comes back when this closes.
    #[serde(skip)]
    pub back: bool,
    /// The dialog that comes back when this closes (Table of Contents › Modify…).
    #[serde(skip)]
    pub resume: Option<Box<Dialog>>,
    pub id: String,
    pub name: String,
    /// Style for following paragraph (a style id).
    pub next: String,
    /// The changes so far, as `styles.modify` takes them: only what the user set, so what the
    /// style inherits stays inherited.
    pub chr: CharProps,
    pub para: ParaProps,
}

impl ModifyForm {
    pub fn open(app: &WordApp, id: &str) -> Option<ModifyForm> {
        let st = app.session.doc.styles.get(id)?;
        Some(ModifyForm {
            back: false,
            resume: None,
            id: id.into(),
            name: st.name.clone(),
            next: st.next.clone().unwrap_or_else(|| id.into()),
            chr: CharProps::default(),
            para: ParaProps::default(),
        })
    }

    /// Is it a character style (no paragraph formatting)?
    fn character(&self, app: &WordApp) -> bool {
        app.session.doc.styles.get(&self.id).is_some_and(|st| st.kind == StyleKind::Character)
    }

    /// How the style's text looks with the changes so far.
    pub fn resolved_char(&self, app: &WordApp) -> ResolvedChar {
        let character = self.character(app);
        let chr = CharProps { style: character.then(|| self.id.clone()), ..self.chr.clone() };
        app.session.doc.styles.resolve_char((!character).then_some(self.id.as_str()), &chr)
    }

    /// How the style's paragraphs look with the changes so far.
    pub fn resolved_para(&self, app: &WordApp) -> ResolvedPara {
        app.session.doc.styles.resolve_para(&ParaProps { style: Some(self.id.clone()), ..self.para.clone() })
    }

    /// OK: every change as one `styles.modify`; nothing (and no undo step) when nothing changed.
    pub(crate) fn apply(&self, app: &mut WordApp) {
        let Some(st) = app.session.doc.styles.get(&self.id) else { return };
        let next_was = st.next.clone().unwrap_or_else(|| self.id.clone());
        let unchanged = self.name == st.name && self.chr == CharProps::default() && self.para == ParaProps::default();
        if unchanged && self.next == next_was {
            return;
        }
        let mut v = json!({"style": self.id, "name": self.name, "chr": self.chr, "para": self.para});
        // Only a changed following style: an unset one means the style itself.
        if self.next != next_was {
            v["next"] = json!(self.next);
        }
        let _ = app.run("styles.modify", v);
    }
}

/// Returns true to close.
pub fn body(app: &mut WordApp, ui: &mut Ui, f: &mut ModifyForm) -> bool {
    let Some(st) = app.session.doc.styles.get(&f.id).cloned() else { return true };
    let character = st.kind == StyleKind::Character;
    let (rc, rp) = (f.resolved_char(app), f.resolved_para(app));
    ui.label(egui::RichText::new(tl!("Properties")).font(semibold(12.5)));
    egui::Grid::new("ms_props").num_columns(2).spacing(vec2(10.0, 6.0)).show(ui, |ui| {
        ui.label(tl!("Name:"));
        ui.add(egui::TextEdit::singleline(&mut f.name).desired_width(260.0));
        ui.end_row();
        ui.label(tl!("Style type:"));
        ui.label(tl!(style_type(&st).1));
        ui.end_row();
        ui.label(tl!("Style based on:"));
        let based = st.based_on.as_deref().and_then(|b| app.session.doc.styles.get(b)).map(|b| b.name.clone());
        ui.label(based.unwrap_or_else(|| tl!("(no style)").to_string()));
        ui.end_row();
        if !character {
            ui.label(tl!("Style for following paragraph:"));
            next_style_combo(app, ui, &mut f.next);
            ui.end_row();
        }
    });
    ui.add_space(4.0);
    ui.label(egui::RichText::new(tl!("Formatting")).font(semibold(12.5)));
    ui.horizontal(|ui| font_row(app, ui, f, &rc));
    if !character {
        ui.horizontal(|ui| para_row(ui, f, &rp));
    }
    // Character spacing here too, not only in Format › Font… › Advanced.
    ui.horizontal(|ui| {
        let mut spacing = rc.spacing;
        let names = [tl!("Normal"), tl!("Expanded"), tl!("Condensed")];
        crate::dialogs::signed_by(ui, "ms_spacing", tl!("Character spacing:"), &mut spacing, names, 1.0);
        if spacing != rc.spacing {
            f.chr.spacing = Some(spacing);
        }
    });
    ui.add_space(4.0);
    crate::previews::style_sample(app, ui, &f.id, &f.chr, &f.para, vec2(440.0, 120.0));
    ui.add_space(4.0);
    ui.add(egui::Label::new(egui::RichText::new(description(app, &st, f)).small()).wrap());
    let mut more = None;
    let (ok, cancel) = buttons_with(ui, tl!("OK"), |ui| {
        ui.menu_button(tl!("Format"), |ui| {
            if ui.button(tl!("Font…")).clicked() {
                more = Some("font");
                ui.close();
            }
            if !character && ui.button(tl!("Paragraph…")).clicked() {
                more = Some("paragraph");
                ui.close();
            }
        });
    });
    // Format › Font… / Paragraph…: that dialog takes over and brings this one back.
    if let Some(which) = more {
        app.dialog = Some(Dialog::for_style(app, which, f.clone()));
        return true;
    }
    if ok {
        f.apply(app);
    }
    if (ok || cancel) && f.back {
        app.dialog = Some(Dialog::ManageStyles { alphabetical: false, selected: f.id.clone() });
    }
    if (ok || cancel)
        && let Some(r) = f.resume.take()
    {
        app.dialog = Some(*r);
    }
    ok || cancel
}

/// Font, size, bold, italic, underline and colour.
fn font_row(app: &mut WordApp, ui: &mut Ui, f: &mut ModifyForm, rc: &ResolvedChar) {
    let shown = app.previews.font_shown(&rc.font);
    let fams = app.previews.families();
    let prev = app.previews.font_preview_fn();
    if let Some(pick) = crate::widgets::font_combo(ui, "ms_font", 180.0, &shown, &fams, Some(&*prev)) {
        if pick == crate::widgets::ADD_FONT {
            let _ = app.run("font.add", json!({}));
        } else if pick == crate::widgets::LOAD_SYSTEM_FONTS {
            let _ = app.run("font.loadSystem", json!({}));
        } else {
            // The Latin font serves Persian and Arabic text too, as the Font dialog sets it.
            let font = app.previews.font_stored(&pick);
            f.chr.font_cs = Some(font.clone());
            f.chr.font = Some(font);
        }
    }
    let mut size = rc.size;
    if ui.add(egui::DragValue::new(&mut size).range(1.0..=1638.0).speed(0.5).max_decimals(1)).on_hover_text(tl!("Font Size")).changed() {
        f.chr.size = Some(size);
    }
    if toggle(ui, "bold", tl!("Bold"), rc.bold).clicked() {
        f.chr.bold = Some(!rc.bold);
    }
    if toggle(ui, "italic", tl!("Italic"), rc.italic).clicked() {
        f.chr.italic = Some(!rc.italic);
    }
    let underlined = rc.underline != Underline::None;
    if toggle(ui, "underline", tl!("Underline"), underlined).clicked() {
        f.chr.underline = Some(if underlined { Underline::None } else { Underline::Single });
    }
    let mut color = match rc.color {
        TextColor::Rgb(c) => c.hex(),
        TextColor::Auto => "auto".into(),
    };
    let was = color.clone();
    let theme = app.session.doc.settings.theme_colors.clone();
    color_field(ui, &theme, &mut color, "Automatic", "auto");
    if color != was {
        f.chr.color = Rgb::parse(&color).map(TextColor::Rgb).or(color.eq_ignore_ascii_case("auto").then_some(TextColor::Auto));
    }
}

/// Alignment, line spacing, and space before and after.
fn para_row(ui: &mut Ui, f: &mut ModifyForm, rp: &ResolvedPara) {
    // Alignment as seen on the page; the style stores it from its start edge.
    let shown = rp.align.visual(rp.bidi);
    for (a, icon, tip) in [
        (Align::Left, "alignLeft", "Align Left"),
        (Align::Center, "alignCenter", "Center"),
        (Align::Right, "alignRight", "Align Right"),
        (Align::Justify, "justify", "Justify"),
    ] {
        if toggle(ui, icon, tl!(tip), shown == a).clicked() {
            f.para.align = Some(a.visual(rp.bidi));
        }
    }
    ui.separator();
    // The Paragraph dialog's names for them.
    for (m, label, tip) in [(1.0, "1.0", "Single spacing"), (1.5, "1.5", "1.5 lines"), (2.0, "2.0", "Double spacing")] {
        if ui.selectable_label(rp.line_spacing == LineSpacing::Multiple(m), label).on_hover_text(tl!(tip)).clicked() {
            f.para.line_spacing = Some(LineSpacing::Multiple(m));
        }
    }
    ui.separator();
    let pt = format!(" {}", tl!("pt"));
    for (label, value, set) in [(tl!("Before:"), rp.space_before, &mut f.para.space_before), (tl!("After:"), rp.space_after, &mut f.para.space_after)]
    {
        ui.label(label);
        let mut v = value;
        if ui.add(egui::DragValue::new(&mut v).range(0.0..=1584.0).speed(1.0).suffix(pt.as_str())).changed() {
            *set = Some(v);
        }
    }
}

/// A 22 px icon button drawn on when `on`, like the ribbon's toggles.
fn toggle(ui: &mut Ui, icon: &str, tip: &str, on: bool) -> egui::Response {
    let t = Tokens::get(ui.ctx());
    let (r, resp) = ui.allocate_exact_size(vec2(24.0, 22.0), egui::Sense::click());
    crate::widgets::bg(ui, r, &resp, on, &t);
    crate::icons::paint(ui.painter(), egui::Rect::from_center_size(r.center(), vec2(17.0, 17.0)), icon, t.icon, t.accent);
    resp.widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::Button, true, on, tip));
    resp.on_hover_text(tip)
}

/// Style for following paragraph: the paragraph styles by name.
fn next_style_combo(app: &WordApp, ui: &mut Ui, next: &mut String) {
    let styles = &app.session.doc.styles;
    let mut list: Vec<(&str, &str)> =
        styles.styles.iter().filter(|s| s.kind == StyleKind::Paragraph && !s.hidden).map(|s| (s.id.as_str(), s.name.as_str())).collect();
    list.sort_by_cached_key(|(_, name)| name.to_lowercase());
    let shown = styles.get(next).map_or(next.as_str(), |s| s.name.as_str());
    egui::ComboBox::from_id_salt("ms_next").width(260.0).selected_text(shown).show_ui(ui, |ui| {
        for (id, name) in list {
            ui.selectable_value(next, id.to_string(), name);
        }
    });
}

/// Paragraph, character or linked, as Manage Styles names it (its `type`, and the description's
/// words).
fn style_type(st: &Style) -> (&'static str, &'static str) {
    match (st.kind, st.linked.is_some()) {
        (StyleKind::Character, _) => ("character", "Character style"),
        (StyleKind::Paragraph, true) => ("linked", "Linked style (paragraph and character)"),
        _ => ("paragraph", "Paragraph style"),
    }
}

/// What the style will set once OK is pressed, worded as Manage Styles describes styles.
fn description(app: &WordApp, st: &Style, f: &ModifyForm) -> String {
    let mut changed = st.clone();
    changed.chr.overlay(&f.chr);
    changed.para.overlay(&f.para);
    let ty = style_type(st).0;
    let based = st.based_on.as_deref().and_then(|b| app.session.doc.styles.get(b)).map(|b| b.name.clone());
    describe(&json!({"type": ty, "builtIn": st.builtin, "basedOn": based, "format": wordcraft_engine::cmd::para::own_format(&changed)}))
}
