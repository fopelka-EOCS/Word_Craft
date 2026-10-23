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

/// A modal dialog is open, or a dialog [`keyboard`] hasn't seen yet: a shortcut among this
/// frame's keys opened it, modal or modeless, and the keys after it are the dialog's, which takes
/// the keyboard as [`keyboard`] sees it. Dialogs are compared by name, as there, so Mod+H on the
/// Find tab counts too (it turns to Replace).
pub fn opened_unseen(app: &WordApp, ctx: &egui::Context) -> bool {
    open(app) || app.dialog.as_ref().is_some_and(|d| Some(d.name()) != last_shown(ctx))
}

/// The dialog [`keyboard`] saw last.
fn last_shown(ctx: &egui::Context) -> Option<&'static str> {
    ctx.data(|d| d.get_temp::<Option<&'static str>>(shown_id())).flatten()
}

/// Whether this frame's keys are taken: a modal dialog was open at the start of this frame's
/// dialogs (one that has just closed on a key, Mod+Enter being also Enter, its OK, still took
/// that key), or the page read them before a dialog took the keyboard from it.
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
    let was = last_shown(ctx);
    ctx.data_mut(|d| d.insert_temp(shown_id(), shown));
    let Some((page, _)) = app.canvas.widget_id else { return };
    let page_focused = ctx.memory(|m| m.has_focus(page));
    if page_focused && (modal || (shown.is_some() && shown != was)) {
        ctx.memory_mut(|m| m.surrender_focus(page));
        // The page reads next frame's keys before it looks at its focus again: it has lost the
        // keyboard now, or a key right after a modeless dialog's shortcut still reaches it. It read
        // this frame's keys already, so no shortcut after the dialogs runs them again.
        app.canvas.focused = false;
        let pass = ctx.cumulative_pass_nr();
        ctx.data_mut(|d| {
            d.insert_temp(took_focus_id(), true);
            d.insert_temp(held_id(), pass);
        });
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
    // Also in the frame the dialog took the keyboard from the page: the page has read the keys
    // before the shortcut that opened it (Escape, Enter), and the dialog doesn't see them again.
    if !modeless(d) || !(app.canvas.focused || held_this_frame(ctx)) {
        return Vec::new();
    }
    ctx.input_mut(|i| {
        let (keys, rest) = std::mem::take(&mut i.events).into_iter().partition(|e| matches!(e, egui::Event::Key { .. }));
        i.events = rest;
        keys
    })
}

/// After the dialog's body: puts back the keys [`keys_for_page`] took out, for what runs after the
/// dialogs, and settles who has the keyboard next frame, which the page reads before it looks at
/// its focus again. A field of the dialog that has just taken the keyboard from the page (one
/// that takes it as the dialog opens, opened again by its shortcut) has it then, as in
/// [`keyboard`]; once the dialog closes by its own buttons or keys (`closed`), with no other
/// dialog in its place, the page has it, so the first key typed after OK isn't lost.
pub fn give_back(app: &mut WordApp, ctx: &egui::Context, keys: Vec<egui::Event>, closed: bool) {
    if !keys.is_empty() {
        ctx.input_mut(|i| i.events.extend(keys));
    }
    let Some((page, pass)) = app.canvas.widget_id else { return };
    let (page_focused, other_focused) = ctx.memory(|m| (m.has_focus(page), m.focused().is_some_and(|f| f != page)));
    // Only a page this pass drew: behind the File page (Options › Customize Keyboard, About) the
    // page isn't drawn and wouldn't look at its focus again, leaving the shortcuts off there.
    let page_shown = pass == ctx.cumulative_pass_nr() && !app.ui.backstage;
    if closed && page_shown && app.dialog.is_none() && app.colors.is_none() && !app.file_dialog_open() {
        app.canvas.focused = true;
    } else if app.canvas.focused && !page_focused && other_focused {
        app.canvas.focused = false;
        let pass = ctx.cumulative_pass_nr();
        ctx.data_mut(|d| d.insert_temp(held_id(), pass));
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

    /// Keys that come in the frames right after the dialog's shortcut (`Harness::event` gives each
    /// its own frame; the same frame is `a_letter_in_the_shortcuts_own_frame_goes_to_the_dialog`)
    /// are the dialog's: Enter is its OK, a letter is lost.
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

    /// Keys that arrive together, in one frame (`Harness::event` gives each event a frame of its
    /// own), then two frames for what they opened.
    fn one_frame(h: &mut Harness<'static, WordApp>, events: impl IntoIterator<Item = Event>) {
        h.input_mut().events.extend(events);
        h.step();
        h.run_steps(2);
    }

    fn letter(key: Key, text: &str) -> [Event; 3] {
        let [down, up] = key_events(key, Modifiers::NONE);
        [down, Event::Text(text.into()), up]
    }

    /// A letter in the very frame of the shortcut that opens a dialog is the dialog's, for a
    /// modal dialog (Mod+D, Font) and a modeless one (Mod+H, Find and Replace).
    #[test]
    fn a_letter_in_the_shortcuts_own_frame_goes_to_the_dialog() {
        let mut h = harness();
        one_frame(&mut h, key_events(Key::D, mod_key()).into_iter().chain(letter(Key::X, "x")));
        assert!(matches!(h.state().dialog, Some(Dialog::Font { .. })), "Mod+D opens Font");
        assert_eq!(body(&h), TEXT, "the letter doesn't reach the document behind Font");
        press(&mut h, Key::Escape, Modifiers::NONE);
        assert!(h.state().dialog.is_none());

        one_frame(&mut h, key_events(Key::H, mod_key()).into_iter().chain(letter(Key::X, "x")));
        assert!(matches!(h.state().dialog, Some(Dialog::Find { .. })), "Mod+H opens Find and Replace");
        assert_eq!(body(&h), TEXT, "the letter doesn't reach the document behind Find");
    }

    /// The frame after Mod+H, before the page has looked at its focus again, its keys are Find's:
    /// a letter isn't typed, and Enter doesn't start a paragraph as well as act in the dialog.
    #[test]
    fn keys_in_the_frame_after_a_modeless_dialogs_shortcut_go_to_the_dialog() {
        let mut h = harness();
        h.input_mut().events.extend(key_events(Key::H, mod_key()));
        h.step();
        assert!(matches!(h.state().dialog, Some(Dialog::Find { .. })), "Mod+H opens Find and Replace");
        one_frame(&mut h, letter(Key::X, "x"));
        assert_eq!(body(&h), TEXT, "the letter isn't typed");
        press(&mut h, Key::Escape, Modifiers::NONE);
        assert!(h.state().dialog.is_none(), "Escape closes Find");

        h.input_mut().events.extend(key_events(Key::H, mod_key()));
        h.step();
        one_frame(&mut h, key_events(Key::Enter, Modifiers::NONE));
        assert_eq!(body(&h), TEXT, "Enter starts no paragraph");
    }

    /// Mod+H with Replace open and the page holding the keyboard opens Replace again, whose box
    /// takes the keyboard: a letter in the very next frame goes into the box, not the document.
    #[test]
    fn a_modeless_dialog_opened_again_by_its_shortcut_takes_the_keyboard() {
        let mut h = harness();
        press(&mut h, Key::H, mod_key());
        let p = on_the_page(&h);
        click(&mut h, p);
        assert!(h.state().canvas.focused, "a click on the page gives it the keyboard");
        h.input_mut().events.extend(key_events(Key::H, mod_key()));
        h.step();
        one_frame(&mut h, letter(Key::X, "x"));
        assert_eq!(body(&h), TEXT, "the letter isn't typed");
        assert!(matches!(&h.state().dialog, Some(Dialog::Find { query, .. }) if query.ends_with('x')), "it is in Find what");
    }

    /// The first key typed in the frame right after OK reaches the document.
    #[test]
    fn a_letter_right_after_ok_reaches_the_document() {
        let mut h = harness();
        press(&mut h, Key::D, mod_key());
        h.input_mut().events.extend(key_events(Key::Enter, Modifiers::NONE));
        h.step();
        assert!(h.state().dialog.is_none(), "Enter is OK");
        one_frame(&mut h, letter(Key::Y, "y"));
        assert_eq!(body(&h), format!("{TEXT}y"));
    }

    /// A dialog closed by OK over the File page, where the page isn't drawn, leaves the page
    /// without the keyboard, so the shortcuts that run without it still work there.
    #[test]
    fn ok_over_the_file_page_leaves_the_page_without_the_keyboard() {
        let mut h = harness();
        h.state_mut().ui.backstage = true;
        h.state_mut().canvas.focused = false;
        h.state_mut().run("ui.dialog", json!({"name": "paragraph"})).unwrap();
        h.run_steps(2);
        h.input_mut().events.extend(key_events(Key::Enter, Modifiers::NONE));
        h.step();
        assert!(h.state().dialog.is_none(), "Enter is OK");
        assert!(!h.state().canvas.focused, "the page isn't shown, so it doesn't take the keyboard");
    }

    /// Escape or Enter the page has already handled, in the frame of the shortcut that opens Find,
    /// don't reach Find too: Escape doesn't close it, Enter doesn't find the next match.
    #[test]
    fn keys_the_page_read_before_a_modeless_dialogs_shortcut_dont_reach_the_dialog() {
        let mut h = harness();
        one_frame(&mut h, key_events(Key::Escape, Modifiers::NONE).into_iter().chain(key_events(Key::H, mod_key())));
        assert!(matches!(h.state().dialog, Some(Dialog::Find { .. })), "Escape doesn't close Find as it opens");
        press(&mut h, Key::Escape, Modifiers::NONE);
        assert!(h.state().dialog.is_none());

        one_frame(&mut h, key_events(Key::Enter, Modifiers::NONE).into_iter().chain(key_events(Key::H, mod_key())));
        assert_eq!(body(&h), format!("{TEXT}\n"), "Enter was the page's");
        assert!(matches!(&h.state().dialog, Some(Dialog::Find { message, .. }) if message.is_empty()), "and Find didn't search");
    }

    /// With nothing holding the keyboard, a shortcut in the frame of the one that opened a dialog
    /// doesn't change the document behind it: Mod+D, then Mod+B.
    #[test]
    fn a_shortcut_right_after_a_dialogs_shortcut_doesnt_reach_the_document() {
        let mut h = harness();
        h.state_mut().run("select.all", json!({})).unwrap();
        let (page, _) = h.state().canvas.widget_id.expect("the page is shown");
        h.ctx.memory_mut(|m| m.surrender_focus(page));
        h.run_steps(2);
        let depth = h.state().session.undo_depth();
        one_frame(&mut h, key_events(Key::D, mod_key()).into_iter().chain(key_events(Key::B, mod_key())));
        assert!(matches!(h.state().dialog, Some(Dialog::Font { .. })), "Mod+D opens Font");
        assert_eq!(h.state().session.undo_depth(), depth, "Mod+B formatted nothing");
    }

    /// The page read the frame's keys before Mark Index Entry (Alt+Shift+X) took the keyboard:
    /// the shortcuts that run without the page don't read them again, so Mod+B after it formats
    /// nothing (Mark Index Entry asks for no field's focus, so nothing else holds the keyboard).
    #[test]
    fn the_pages_keys_arent_read_again_once_a_dialog_takes_the_keyboard() {
        if cfg!(target_os = "macos") {
            return; // Alt+Shift+X is the key everywhere else.
        }
        let mut h = harness();
        h.state_mut().run("select.all", json!({})).unwrap();
        h.run_steps(2);
        assert!(h.state().canvas.focused);
        let depth = h.state().session.undo_depth();
        let alt_shift = Modifiers { alt: true, shift: true, ..Modifiers::NONE };
        one_frame(&mut h, key_events(Key::X, alt_shift).into_iter().chain(key_events(Key::B, mod_key())));
        assert!(matches!(h.state().dialog, Some(Dialog::Refs(_))), "Alt+Shift+X opens Mark Index Entry");
        assert_eq!(h.state().session.undo_depth(), depth, "Mod+B formatted nothing");
    }

    /// Go To's page box takes the keyboard as the tab opens, but a click on the page gives the
    /// page the keyboard with the dialog still open: the box doesn't take it back.
    #[test]
    fn the_go_to_box_leaves_the_keyboard_to_the_page() {
        let mut h = harness();
        h.state_mut().run("ui.dialog", json!({"name": "goto"})).unwrap();
        h.run_steps(3);
        assert!(!h.state().canvas.focused, "Go To takes the keyboard");
        type_letter(&mut h, Key::Num2, "2");
        assert!(matches!(&h.state().dialog, Some(Dialog::Find { goto_target, .. }) if goto_target == "2"), "the digit is in the box");
        let p = on_the_page(&h);
        let dialog = h.ctx.memory(|m| m.area_rect(egui::Id::new(("dialog", "Find and Replace")))).expect("the dialog is shown");
        assert!(!dialog.contains(p), "{p:?} is outside the dialog {dialog:?}");
        click(&mut h, p);
        h.run_steps(2);
        assert!(h.state().canvas.focused, "a click on the page gives it the keyboard");
        type_letter(&mut h, Key::Y, "y");
        assert!(h.state().dialog.is_some(), "Go To stays open");
        assert_eq!(body(&h), format!("{TEXT}y"), "the letter is typed into the document");
    }

    /// With Find open and the page holding the keyboard, a click on the Go To tab gives its page
    /// box the keyboard: a digit goes into the box, not into the document.
    #[test]
    fn choosing_the_go_to_tab_gives_its_box_the_keyboard() {
        use egui_kittest::kittest::Queryable;
        let mut h = harness();
        h.state_mut().run("ui.dialog", json!({"name": "find"})).unwrap();
        h.run_steps(3);
        let p = on_the_page(&h);
        click(&mut h, p);
        assert!(h.state().canvas.focused, "a click on the page gives it the keyboard");
        let dialog = h.ctx.memory(|m| m.area_rect(egui::Id::new(("dialog", "Find and Replace")))).expect("the dialog is shown");
        let tab = h.query_all_by_label("Go To").find(|n| dialog.contains(n.rect().center())).expect("the Go To tab").rect().center();
        click(&mut h, tab);
        assert!(!h.state().canvas.focused, "the click on the tab takes the keyboard from the page");
        assert!(matches!(&h.state().dialog, Some(Dialog::Find { tab: crate::dialogs::FindTab::GoTo, .. })), "the Go To tab is shown");
        type_letter(&mut h, Key::Num2, "2");
        assert!(matches!(&h.state().dialog, Some(Dialog::Find { goto_target, .. }) if goto_target == "2"), "the digit is in the box");
        assert_eq!(body(&h), TEXT, "and not in the document");
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

    /// A click on the page in the frame a modal dialog opens in (its shortcut and the click arrive
    /// together; the page is drawn before the dialog and its backdrop) doesn't move the caret, and
    /// a right-click there opens no menu behind the dialog.
    #[test]
    fn a_click_in_the_frame_a_modal_dialog_opens_in_does_nothing() {
        let mut h = harness();
        let p = on_the_page(&h);
        for button in [egui::PointerButton::Primary, egui::PointerButton::Secondary] {
            h.state_mut().run("caret.docStart", json!({})).unwrap();
            h.event(Event::PointerMoved(p));
            h.run_steps(2);
            let start = format!("{:?}", h.state().session.sel);
            let button_event = |pressed| Event::PointerButton { pos: p, button, pressed, modifiers: Modifiers::NONE };
            one_frame(&mut h, key_events(Key::D, mod_key()).into_iter().chain([button_event(true), button_event(false)]));
            assert!(matches!(h.state().dialog, Some(Dialog::Font { .. })), "{button:?}: Mod+D opens Font");
            assert_eq!(format!("{:?}", h.state().session.sel), start, "{button:?}: the caret stays");
            assert!(!h.state().canvas.context_menu_open, "{button:?}: no menu behind the dialog");
            press(&mut h, Key::Escape, Modifiers::NONE);
            assert!(h.state().dialog.is_none(), "Escape cancels");
        }
    }

    /// A click on an equation's options strip in the frame a modal dialog opens in (opened from
    /// outside, by an agent, just before) opens no equation menu behind the dialog.
    #[test]
    fn the_equation_menu_doesnt_open_behind_a_modal_dialog() {
        let mut h = harness();
        h.state_mut().run("insert.equation", json!({})).unwrap();
        h.run_steps(3);
        let strip = h.state().canvas.eq_options.expect("the equation's frame is shown").strip.center();
        h.event(Event::PointerMoved(strip));
        h.run_steps(2);
        h.state_mut().run("ui.dialog", json!({"name": "paragraph"})).unwrap();
        let click = |pressed| Event::PointerButton { pos: strip, button: egui::PointerButton::Primary, pressed, modifiers: Modifiers::NONE };
        one_frame(&mut h, [click(true), click(false)]);
        assert!(h.state().dialog.is_some());
        assert!(!h.state().canvas.context_menu_open, "no equation menu behind the dialog");
    }

    /// With Draw Table on, a drag on the page from `from` to `to`, with `before_release` in the
    /// frame before the button comes up.
    fn draw_stroke(h: &mut Harness<'static, WordApp>, from: egui::Pos2, to: egui::Pos2, before_release: impl IntoIterator<Item = Event>) {
        let button = |pos, pressed| Event::PointerButton { pos, button: egui::PointerButton::Primary, pressed, modifiers: Modifiers::NONE };
        h.event(Event::PointerMoved(from));
        h.step();
        h.event(button(from, true));
        h.step();
        h.event(Event::PointerMoved(to));
        h.step();
        one_frame(h, before_release);
        h.event(button(to, false));
        h.run_steps(2);
    }

    /// A drag on the page still going when a modal dialog opens (its shortcut pressed with the
    /// button down) is dropped, as Escape drops it: releasing the button over the dialog and
    /// closing the dialog draw nothing. Without the dialog the same drag draws a table.
    #[test]
    fn a_drag_on_the_page_ends_when_a_modal_dialog_opens() {
        let mut h = harness();
        h.state_mut().run("table.draw", json!({"on": true})).unwrap();
        h.run_steps(2);
        let to = on_the_page(&h);
        let from = to - egui::vec2(200.0, 150.0);
        let depth = h.state().session.undo_depth();
        draw_stroke(&mut h, from, to, []);
        assert_eq!(h.state().session.undo_depth(), depth + 1, "without a dialog the drag draws a table");
        h.state_mut().run("edit.undo", json!({})).unwrap();
        h.run_steps(2);

        let depth = h.state().session.undo_depth();
        draw_stroke(&mut h, from, to, key_events(Key::D, mod_key()));
        assert!(matches!(h.state().dialog, Some(Dialog::Font { .. })), "Mod+D opens Font");
        press(&mut h, Key::Escape, Modifiers::NONE);
        assert!(h.state().dialog.is_none(), "Escape cancels");
        h.run_steps(2);
        assert_eq!(h.state().session.undo_depth(), depth, "no table is drawn");
        assert!(h.state().canvas.table_stroke.is_none(), "the stroke is dropped");
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
