//! Dialogs and the keyboard. Every dialog takes the keyboard from the page as it opens, so Tab
//! moves through the dialog and Enter is only its default button (it no longer also starts a
//! new paragraph). A modal dialog keeps the document behind it from taking any key or click
//! until it closes, and then the page gets the keyboard back. Find and Replace, Symbol,
//! Cross-reference and Mark Index Entry are modeless: a click on the page gives it the keyboard
//! with the dialog open, and while the page has it, Enter and Escape are the document's, not the
//! dialog's.

use crate::WordApp;
use crate::dialogs::Dialog;
use crate::dialogs_insert::InsertDialog;
use crate::dialogs_refs::RefsDialog;

/// Whether the document can be edited while `d` is open.
pub fn modeless(d: &Dialog) -> bool {
    match d {
        Dialog::Find { .. } => true,
        Dialog::Insert(d) => matches!(**d, InsertDialog::Symbol(_)),
        Dialog::Refs(d) => matches!(**d, RefsDialog::CrossRef(_) | RefsDialog::MarkEntry(_)),
        _ => false,
    }
}

/// A modal dialog is open: the Colors dialog, or any dialog but the modeless ones.
pub fn open(app: &WordApp) -> bool {
    app.colors.is_some() || app.dialog.as_ref().is_some_and(|d| !modeless(d))
}

/// Whether a modal dialog was open at the start of this frame's dialogs: one that has just closed
/// on a key (Mod+Enter is also Enter, its OK) still took that key.
pub fn held_this_frame(ctx: &egui::Context) -> bool {
    let pass = ctx.cumulative_pass_nr();
    ctx.data(|d| d.get_temp::<u64>(held_id())) == Some(pass)
}

fn held_id() -> egui::Id {
    egui::Id::new("wordcraft.modalHeld")
}

fn shown_id() -> egui::Id {
    egui::Id::new("wordcraft.dialogShown")
}

fn took_focus_id() -> egui::Id {
    egui::Id::new("wordcraft.dialogTookFocus")
}

/// Runs every frame, after the page and before the dialogs. A dialog takes the keyboard from the
/// page as it opens, a modal one for as long as it is open. Once no dialog is open, the page gets
/// the keyboard back unless something else has it, also when the dialog closed some other way
/// than its own buttons (a closed dialog's field keeps the focus until the frame ends).
pub fn keyboard(app: &mut WordApp, ctx: &egui::Context) {
    let modal = open(app);
    if modal {
        let pass = ctx.cumulative_pass_nr();
        ctx.data_mut(|d| d.insert_temp(held_id(), pass));
    }
    let shown = app.dialog.as_ref().map(Dialog::name);
    let was = ctx.data(|d| d.get_temp::<Option<&'static str>>(shown_id())).flatten();
    ctx.data_mut(|d| d.insert_temp(shown_id(), shown));
    let Some((page, _)) = app.canvas.widget_id else { return };
    let page_focused = ctx.memory(|m| m.has_focus(page));
    if page_focused && (modal || (shown.is_some() && shown != was)) {
        ctx.memory_mut(|m| m.surrender_focus(page));
        ctx.data_mut(|d| d.insert_temp(took_focus_id(), true));
    } else if shown.is_none() && app.colors.is_none() && ctx.data(|d| d.get_temp::<bool>(took_focus_id())).is_some() {
        if !page_focused && ctx.memory(|m| m.focused().is_none()) {
            app.canvas.want_focus = true;
        }
        if page_focused || app.canvas.want_focus {
            ctx.data_mut(|d| d.remove::<bool>(took_focus_id()));
        }
    }
}

/// Keeps clicks and the keyboard within the modal dialog `d`, shown on `layer`. An invisible
/// backdrop over the whole window, behind every window (as `egui::Modal` has), takes the clicks,
/// drags and hovering meant for the page, the ribbon and the panes. Tab stays within the dialog,
/// except while a file browser opened from it is up: that is another window, above the dialog.
pub fn block_behind(app: &WordApp, ctx: &egui::Context, d: &Dialog, layer: egui::LayerId) {
    if modeless(d) {
        return;
    }
    let rect = ctx.content_rect();
    egui::Area::new(egui::Id::new("wordcraft.modalBackdrop"))
        .order(egui::Order::Background)
        .fixed_pos(rect.min)
        .movable(false)
        .interactable(true)
        .show(ctx, |ui| {
            let _backdrop = ui.allocate_response(rect.size(), egui::Sense::click_and_drag());
        });
    if !app.file_dialog_open() {
        ctx.memory_mut(|m| m.set_modal_layer(layer));
    }
}

/// While the modeless dialog `d` is open and the page has the keyboard, the keys are the
/// document's: the dialog doesn't see them (Enter starts a paragraph rather than finding the next
/// match, and Escape doesn't close the dialog). Returns the keys taken out, for [`give_back`].
pub fn keys_for_page(app: &WordApp, ctx: &egui::Context, d: &Dialog) -> Vec<egui::Event> {
    if !modeless(d) || !app.canvas.focused {
        return Vec::new();
    }
    ctx.input_mut(|i| {
        let (keys, rest) = std::mem::take(&mut i.events).into_iter().partition(|e| matches!(e, egui::Event::Key { .. }));
        i.events = rest;
        keys
    })
}

/// Puts back the keys [`keys_for_page`] took out, for what runs after the dialogs.
pub fn give_back(ctx: &egui::Context, keys: Vec<egui::Event>) {
    if !keys.is_empty() {
        ctx.input_mut(|i| i.events.extend(keys));
    }
}

#[cfg(test)]
mod tests {
    use egui::{Event, Key, Modifiers};
    use egui_kittest::Harness;
    use serde_json::json;
    use wordcraft_doc::StoryRef;

    use crate::WordApp;
    use crate::dialogs::Dialog;

    const TEXT: &str = "Ala ma kota";

    fn new_app() -> WordApp {
        WordApp::new(wordcraft_engine::Session::new(wordcraft_doc::Document::new()), crate::Services::default())
    }

    /// A window with `TEXT` typed, the caret at its end and the page holding the keyboard.
    fn harness() -> Harness<'static, WordApp> {
        let mut a = new_app();
        a.run("text.insert", json!({"text": TEXT})).unwrap();
        let mut h = Harness::builder().with_size(egui::vec2(1440.0, 900.0)).build_ui_state(
            |ui, app: &mut WordApp| {
                let ctx = ui.ctx().clone();
                app.logic(&ctx);
                app.ui(ui);
            },
            a,
        );
        h.run_steps(6);
        assert!(h.state().canvas.focused, "the page has the keyboard");
        h
    }

    fn key_events(key: Key, modifiers: Modifiers) -> [Event; 2] {
        let e = |pressed| Event::Key { key, physical_key: None, pressed, repeat: false, modifiers };
        [e(true), e(false)]
    }

    /// A key pressed and released in one frame, then a frame for what it opened or closed.
    fn press(h: &mut Harness<'static, WordApp>, key: Key, modifiers: Modifiers) {
        for e in key_events(key, modifiers) {
            h.event(e);
        }
        h.run_steps(2);
    }

    /// A letter as the keyboard sends it: the key, then its text.
    fn type_letter(h: &mut Harness<'static, WordApp>, key: Key, text: &str) {
        let [down, up] = key_events(key, Modifiers::NONE);
        h.event(down);
        h.event(Event::Text(text.into()));
        h.event(up);
        h.run_steps(2);
    }

    fn mod_key() -> Modifiers {
        if cfg!(target_os = "macos") { Modifiers::MAC_CMD | Modifiers::COMMAND } else { Modifiers::CTRL | Modifiers::COMMAND }
    }

    fn body(h: &Harness<'static, WordApp>) -> String {
        h.state().session.doc.plain_text(StoryRef::Body)
    }

    fn click(h: &mut Harness<'static, WordApp>, p: egui::Pos2) {
        h.event(Event::PointerMoved(p));
        h.step();
        let button = |pressed| Event::PointerButton { pos: p, button: egui::PointerButton::Primary, pressed, modifiers: Modifiers::NONE };
        h.event(button(true));
        h.step();
        h.event(button(false));
        h.run_steps(2);
    }

    /// A point on the first page, below the text and the dialogs.
    fn on_the_page(h: &Harness<'static, WordApp>) -> egui::Pos2 {
        let view = h.state().canvas.viewport.expect("the page is shown");
        egui::pos2(view.center().x, view.max.y - 30.0)
    }

    /// Font, opened with its shortcut while the page has the keyboard: a letter, Tab and Enter
    /// go to the dialog, and Enter is its OK alone (no new paragraph behind it). Tab moves into
    /// the dialog. Once it closes, typing goes into the document again, after Escape too.
    #[test]
    fn a_modal_dialog_takes_every_key_until_it_closes() {
        let mut h = harness();
        press(&mut h, Key::D, mod_key());
        assert!(matches!(h.state().dialog, Some(Dialog::Font { .. })), "Mod+D opens Font");
        type_letter(&mut h, Key::X, "x");
        assert_eq!(body(&h), TEXT, "a letter doesn't reach the document");
        press(&mut h, Key::Enter, Modifiers::NONE);
        assert!(h.state().dialog.is_none(), "Enter is OK");
        assert_eq!(body(&h), TEXT, "and only OK: no new paragraph");
        type_letter(&mut h, Key::Y, "y");
        assert_eq!(body(&h), format!("{TEXT}y"), "the page has the keyboard back");

        press(&mut h, Key::D, mod_key());
        press(&mut h, Key::Tab, Modifiers::NONE);
        assert!(h.state().dialog.is_some(), "Tab doesn't close the dialog");
        assert_eq!(body(&h), format!("{TEXT}y"), "Tab doesn't reach the document");
        let focused = h.ctx.memory(|m| m.focused()).expect("Tab focuses a field");
        let layer = h.ctx.read_response(focused).map(|r| r.layer_id.id);
        assert_eq!(layer, Some(egui::Id::new(("dialog", "Font"))), "the field is the dialog's");
        press(&mut h, Key::Escape, Modifiers::NONE);
        assert!(h.state().dialog.is_none(), "Escape cancels");
        type_letter(&mut h, Key::Z, "z");
        assert_eq!(body(&h), format!("{TEXT}yz"));
    }

    /// Keys that come right after the dialog's shortcut, in the same frame or the next, before
    /// the page has given up the keyboard, are the dialog's: Enter is its OK, a letter is lost.
    #[test]
    fn keys_right_after_the_shortcut_go_to_the_dialog() {
        let mut h = harness();
        let [d_down, d_up] = key_events(Key::D, mod_key());
        let [x_down, x_up] = key_events(Key::X, Modifiers::NONE);
        let [enter_down, enter_up] = key_events(Key::Enter, Modifiers::NONE);
        for e in [d_down, d_up, x_down, Event::Text("x".into()), x_up, enter_down, enter_up] {
            h.event(e);
        }
        h.run_steps(2);
        assert!(h.state().dialog.is_none(), "Enter is the dialog's OK");
        assert_eq!(body(&h), TEXT, "neither the letter nor Enter reach the document");

        for e in key_events(Key::D, mod_key()) {
            h.event(e);
        }
        h.step();
        type_letter(&mut h, Key::X, "x");
        assert!(h.state().dialog.is_some());
        assert_eq!(body(&h), TEXT, "a letter in the next frame doesn't either");
    }

    /// Mod+Enter in a modal dialog is its OK (Enter), and the document behind it gets no page
    /// break although the dialog has closed by the time the shortcuts run.
    #[test]
    fn mod_enter_in_a_modal_dialog_is_only_its_ok() {
        let mut h = harness();
        press(&mut h, Key::D, mod_key());
        press(&mut h, Key::Enter, mod_key());
        assert!(h.state().dialog.is_none(), "Mod+Enter is OK");
        assert_eq!(body(&h), TEXT, "no page break");
    }

    /// Mod+B formats the selection while nothing has the keyboard, but not behind a modal dialog.
    #[test]
    fn shortcuts_dont_reach_the_document_behind_a_modal_dialog() {
        let mut h = harness();
        h.state_mut().run("select.all", json!({})).unwrap();
        let (page, _) = h.state().canvas.widget_id.expect("the page is shown");
        h.ctx.memory_mut(|m| m.surrender_focus(page));
        h.run_steps(2);
        let depth = h.state().session.undo_depth();
        press(&mut h, Key::B, mod_key());
        assert_eq!(h.state().session.undo_depth(), depth + 1, "without a dialog, Mod+B formats");
        h.state_mut().run("edit.undo", json!({})).unwrap();

        h.state_mut().run("ui.dialog", json!({"name": "paragraph"})).unwrap();
        h.run_steps(2);
        let depth = h.state().session.undo_depth();
        press(&mut h, Key::B, mod_key());
        assert!(h.state().dialog.is_some());
        assert_eq!(h.state().session.undo_depth(), depth, "nothing was formatted");
    }

    /// A click on the page outside a modal dialog doesn't move the caret; without the dialog the
    /// same click does.
    #[test]
    fn a_click_on_the_page_behind_a_modal_dialog_does_nothing() {
        let mut h = harness();
        h.state_mut().run("caret.docStart", json!({})).unwrap();
        h.run_steps(2);
        let start = format!("{:?}", h.state().session.sel);
        let p = on_the_page(&h);
        click(&mut h, p);
        assert_ne!(format!("{:?}", h.state().session.sel), start, "without a dialog the click moves the caret");

        h.state_mut().run("caret.docStart", json!({})).unwrap();
        h.state_mut().run("ui.dialog", json!({"name": "paragraph"})).unwrap();
        h.run_steps(3);
        let dialog = h.ctx.memory(|m| m.area_rect(egui::Id::new(("dialog", "Paragraph")))).expect("the dialog is shown");
        assert!(!dialog.contains(p), "{p:?} is outside the dialog {dialog:?}");
        click(&mut h, p);
        assert!(h.state().dialog.is_some());
        assert_eq!(format!("{:?}", h.state().session.sel), start, "the caret stays");
    }

    /// Find and Replace takes the keyboard as it opens: Enter in its box finds the next match.
    /// A click on the page gives the page the keyboard with Find still open; then Enter starts a
    /// paragraph rather than finding the next match, and Escape leaves the dialog open.
    #[test]
    fn a_modeless_dialog_leaves_enter_and_escape_to_the_page() {
        let mut h = harness();
        h.state_mut().run("ui.dialog", json!({"name": "find"})).unwrap();
        h.run_steps(2);
        assert!(!h.state().canvas.focused, "Find takes the keyboard");
        h.event(Event::Text("kota".into()));
        h.run_steps(2);
        press(&mut h, Key::Enter, Modifiers::NONE);
        let message = |h: &Harness<'static, WordApp>| match &h.state().dialog {
            Some(Dialog::Find { message, .. }) => message.clone(),
            d => panic!("Find closed: {d:?}"),
        };
        let found = message(&h);
        assert!(!found.is_empty(), "Enter in the box finds the next match");
        assert_eq!(body(&h), TEXT);

        let p = on_the_page(&h);
        click(&mut h, p);
        assert!(h.state().canvas.focused, "a click on the page gives it the keyboard");
        press(&mut h, Key::Enter, Modifiers::NONE);
        assert_eq!(body(&h), format!("{TEXT}\n"), "Enter starts a paragraph");
        assert!(h.state().session.sel.is_collapsed(), "and doesn't find the next match, which it would select");
        press(&mut h, Key::Escape, Modifiers::NONE);
        assert!(h.state().dialog.is_some(), "Escape doesn't close it");
        type_letter(&mut h, Key::Y, "y");
        assert_eq!(body(&h), format!("{TEXT}\ny"));
    }

    /// Symbol, opened while the page has the keyboard, takes it: a letter doesn't reach the
    /// document and Escape closes the dialog, which gives the page the keyboard back.
    #[test]
    fn a_modeless_dialog_takes_the_keyboard_as_it_opens() {
        let mut h = harness();
        h.state_mut().run("ui.dialog", json!({"name": "symbol"})).unwrap();
        h.run_steps(2);
        type_letter(&mut h, Key::X, "x");
        assert_eq!(body(&h), TEXT);
        press(&mut h, Key::Escape, Modifiers::NONE);
        assert!(h.state().dialog.is_none(), "Escape closes Symbol");
        type_letter(&mut h, Key::Y, "y");
        assert_eq!(body(&h), format!("{TEXT}y"));
    }

    /// A dialog closed from outside (a script, an agent) with one of its fields focused gives the
    /// page the keyboard back.
    #[test]
    fn the_page_gets_the_keyboard_back_when_a_dialog_closes_from_outside() {
        let mut h = harness();
        press(&mut h, Key::D, mod_key());
        press(&mut h, Key::Tab, Modifiers::NONE);
        assert!(h.ctx.memory(|m| m.focused()).is_some(), "a field of the dialog has the keyboard");
        h.state_mut().dialog = None;
        h.run_steps(3);
        type_letter(&mut h, Key::Y, "y");
        assert_eq!(body(&h), format!("{TEXT}y"));
    }

    /// The Colors dialog is modal too: a letter doesn't reach the document behind it.
    #[test]
    fn the_colors_dialog_takes_every_key() {
        let mut h = harness();
        h.state_mut().colors = Some(crate::colors::ColorsDialog::new(None, crate::colors::Target::cmd("format.fontColor")));
        h.run_steps(2);
        type_letter(&mut h, Key::X, "x");
        assert_eq!(body(&h), TEXT);
        h.state_mut().colors = None;
        h.run_steps(3);
        type_letter(&mut h, Key::Y, "y");
        assert_eq!(body(&h), format!("{TEXT}y"), "the page has the keyboard back");
    }

    #[test]
    fn find_symbol_cross_reference_and_mark_entry_are_modeless() {
        let mut a = new_app();
        for (name, modeless) in [
            ("find", true),
            ("replace", true),
            ("goto", true),
            ("symbol", true),
            ("crossReference", true),
            ("markEntry", true),
            ("specialCharacters", true),
            ("font", false),
            ("paragraph", false),
            ("bookmark", false),
            ("columns", false),
            ("field", false),
            ("index", false),
        ] {
            let d = Dialog::open(name, &mut a).unwrap_or_else(|| panic!("no {name} dialog"));
            assert_eq!(super::modeless(&d), modeless, "{name}");
        }
    }
}
