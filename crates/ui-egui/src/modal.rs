//! Dialogs and the keyboard. Every dialog takes the keyboard from the page as it opens, so Tab
//! moves through the dialog and Enter is only its default button (it no longer also starts a
//! new paragraph). A modal dialog keeps the document behind it from taking any key or click
//! until it closes, and then the page gets the keyboard back. Find and Replace, Symbol,
//! Cross-reference and Mark Index Entry are modeless: a click on the page gives it the keyboard
//! with the dialog open, and while the page has it, Enter and Escape are the document's, not the
//! dialog's.
//!
//! Behind a modal window (a modal dialog, the built-in file browser, the Colors dialog, Zotero's
//! alert: [`top`]) the window is inert, as a system dialog disables the window it belongs to. As a
//! system takes all input from one queue, one event after another, two rules at the frame's edge
//! make that hold however a window opens:
//! - **an input event reaches the app only in a frame whose modal windows are the ones already
//!   drawn** ([`pace_input`], from `WordApp::raw_input_hook`): an event that may open a window (a
//!   shortcut, a click, a KeyTip, a screen reader's action, a paste, a dropped file, the window's
//!   close button) ends its frame's input, and while a new window's backdrop isn't up all input
//!   waits;
//! - **input from outside the window runs after the frame's own** (`WordApp::outside_input`: an
//!   agent's request, Zotero's call, a file dialog's or the browser's save picker's answer, files
//!   from the browser or Finder), so what it opens is up before the next frame's input, and what
//!   went on behind it ends at once ([`end_behind`]).
//!
//! With that, the rest is about the window behind a modal window that is up and drawn:
//! - [`backdrop`]: an invisible backdrop over the whole window lies directly under each modal
//!   window, above everything below it, menus and lists over the page included; as a modal window
//!   opens, drags, a held button ([`end_press`]), gestures, menus and lists behind it end;
//! - [`keys_away`]: the windows below the top one don't see the keys or a screen reader's actions
//!   (which name a widget, so no backdrop stops them), also where they read them themselves: they
//!   come back ([`keys_back`]) just before the top window is drawn, after the page's own windows
//!   (the Read Aloud bar);
//! - [`behind`]: the drop-downs of the windows below, drawn over everything, close.
//!
//! What the page shows over itself (the comment card, SmartArt's Text Pane button, Ink Replay's
//! bar) is on `Order::Middle`, under every modal window and its backdrop, however it comes up.
//! Files dropped on the window are refused while a modal window is open, and closing the window
//! waits for it (`WordApp::raw_input_hook`, `WordApp::close_requested`). A modeless dialog that
//! took the keyboard from the page keeps every key from the document's shortcuts until the page is
//! clicked ([`dialog_has_keyboard`]).

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

/// A modal window is open: any dialog but the modeless ones, the built-in file browser, the
/// Colors dialog, or Zotero's alert (Zotero waits for its answer).
pub fn open(app: &WordApp) -> bool {
    top(app) != Top::None
}

/// The modal window on top, which takes the keys ([`keys_back`]); they are drawn in this order,
/// each over the one before.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Top {
    None,
    Dialog,
    FileBrowser,
    Colors,
    ZoteroAlert,
}

/// The modal window on top.
pub fn top(app: &WordApp) -> Top {
    if app.zotero.alert_open() {
        Top::ZoteroAlert
    } else if app.colors.is_some() {
        Top::Colors
    } else if app.file_browser_open() {
        Top::FileBrowser
    } else if app.dialog.as_ref().is_some_and(|d| !modeless(d)) {
        Top::Dialog
    } else {
        Top::None
    }
}

/// A dialog took the keyboard from the page as it opened ([`keyboard`]), and no press elsewhere
/// has ended that since ([`shown`]): the keys are the dialog's, also those none of its fields
/// takes, so none runs a shortcut on the document (Mod+B right after Mark Index Entry's shortcut).
pub fn dialog_has_keyboard(app: &WordApp, ctx: &egui::Context) -> bool {
    app.dialog.is_some() && !app.canvas.focused && ctx.data(|d| d.get_temp::<bool>(dialog_keys_id())).is_some()
}

fn dialog_keys_id() -> egui::Id {
    egui::Id::new("wordcraft.dialogKeys")
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
/// page as it opens, a modal one (or other modal window) for as long as it is open. Once no dialog
/// or other modal window is open, the page gets
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
            d.insert_temp(dialog_keys_id(), true);
            d.insert_temp(held_id(), pass);
        });
    } else if shown.is_none() {
        ctx.data_mut(|d| d.remove::<bool>(dialog_keys_id()));
    }
    if shown.is_none() && !modal && ctx.data(|d| d.get_temp::<bool>(took_focus_id())).is_some() {
        if !page_focused && ctx.memory(|m| m.focused().is_none()) {
            app.canvas.want_focus = true;
        }
        if page_focused || app.canvas.want_focus {
            ctx.data_mut(|d| d.remove::<bool>(took_focus_id()));
        }
    }
}

/// Most input [`pace_input`] keeps waiting, once the pointer's moves and the wheel's turns in it
/// are merged: past it, the newest input is dropped, as a system drops input an app doesn't take
/// (many keys and clicks in each of many slow frames). Nothing past it reaches the app early.
const MAX_HELD: usize = 1024;

/// The input's boundary: an input event reaches the app only in a frame whose modal windows are
/// the ones already drawn, as a system delivers input one event after another to the windows it
/// already shows (and as Dear ImGui trickles its input queue over frames). Two rules, whatever
/// opens or closes a window:
/// - while a modal window is open that no drawn backdrop covers yet (`settled` false: it opened
///   last frame, or its backdrop shows for the first time and takes no click yet), all input
///   waits, and the frame draws it;
/// - an event that may open or close a window ([`may_open`]) ends its frame's input, with the text
///   its key types (the system sends a key's text right after it: KeyTips take both): what comes
///   after it waits for the next frame.
///
/// What waited comes first, after the `earlier` events this was given last time that no frame has
/// taken yet (a frame of logic alone, with the window hidden, keeps its input for the next one).
/// `typing` is whether the page has the keyboard, with no KeyTips showing. Returns whether some
/// input waits (`WordApp::ui` asks for the next frame then).
pub fn pace_input(events: &mut Vec<egui::Event>, earlier: usize, held: &mut Vec<egui::Event>, settled: bool, typing: bool) -> bool {
    if !held.is_empty() {
        let later = events.split_off(earlier.min(events.len()));
        events.append(held);
        events.extend(later);
    }
    let mut wait = if settled {
        match events.iter().position(|e| may_open(e, typing)) {
            Some(k) => {
                let mut end = k + 1;
                if matches!(events.get(k), Some(egui::Event::Key { pressed: true, .. })) && matches!(events.get(end), Some(egui::Event::Text(_))) {
                    end += 1;
                }
                events.split_off(end.min(events.len()))
            }
            None => Vec::new(),
        }
    } else {
        std::mem::take(events)
    };
    if wait.len() > MAX_HELD {
        merge_moves(&mut wait);
    }
    if wait.len() > MAX_HELD {
        drop_newest(&mut wait);
    }
    *held = wait;
    !held.is_empty()
}

/// Brings input past [`MAX_HELD`] down to it, dropping the newest presses, text and moves first
/// and keeping the releases, so no key or button stays down for the app.
fn drop_newest(events: &mut Vec<egui::Event>) {
    let over = events.len().saturating_sub(MAX_HELD);
    log::warn!("{over} input events dropped: the app is too slow to take them");
    // From the end, the first `over` events that aren't releases go.
    let mut keep = vec![true; events.len()];
    let mut dropped = 0;
    for (k, e) in keep.iter_mut().zip(events.iter()).rev() {
        if dropped == over {
            break;
        }
        if !is_release(Some(e)) {
            *k = false;
            dropped += 1;
        }
    }
    let mut keep = keep.into_iter();
    events.retain(|_| keep.next().unwrap_or(true));
    events.truncate(MAX_HELD);
}

/// A key, button or touch coming up, or the pointer leaving: what ends a press.
fn is_release(e: Option<&egui::Event>) -> bool {
    matches!(
        e,
        Some(
            egui::Event::Key { pressed: false, .. }
                | egui::Event::PointerButton { pressed: false, .. }
                | egui::Event::PointerGone
                | egui::Event::Touch { phase: egui::TouchPhase::End | egui::TouchPhase::Cancel, .. }
        )
    )
}

/// Marks a key press the window sends again while the key is held as a repeat, as egui would
/// later: the native shells send every press as a first one. `down` holds the keys down; a key
/// coming up, and the window losing focus, take them off.
pub fn mark_repeats(events: &mut [egui::Event], down: &mut Vec<egui::Key>) {
    for e in events.iter_mut() {
        match e {
            egui::Event::Key { key, pressed: true, repeat, .. } => {
                if down.contains(key) {
                    *repeat = true;
                } else {
                    down.push(*key);
                }
            }
            // A browser sends no release for a key let go with Command held: Command coming up
            // ends them all, as egui does.
            egui::Event::Key {
                key: egui::Key::ControlLeft | egui::Key::ControlRight | egui::Key::SuperLeft | egui::Key::SuperRight,
                pressed: false,
                ..
            }
            | egui::Event::WindowFocused(false) => down.clear(),
            egui::Event::Key { key, pressed: false, .. } => down.retain(|k| k != key),
            _ => {}
        }
    }
}

/// Merges the pointer's moves and the wheel's turns that come one after another (a fast mouse or a
/// touchpad sends many in a slow frame): in each run, the last position stands for the run, and the
/// raw mouse motion and each kind of wheel turn add up.
fn merge_moves(events: &mut Vec<egui::Event>) {
    let mut merged: Vec<egui::Event> = Vec::with_capacity(events.len());
    // Where the current run of moves starts in `merged`.
    let mut run = 0;
    for e in std::mem::take(events) {
        if !is_move(&e) {
            merged.push(e);
            run = merged.len();
            continue;
        }
        let same = merged.get_mut(run..).and_then(|tail| tail.iter_mut().find(|m| merges_with(m, &e)));
        match (same, e) {
            (Some(egui::Event::PointerMoved(last)), egui::Event::PointerMoved(p)) => *last = p,
            (Some(egui::Event::MouseMoved(total)), egui::Event::MouseMoved(d)) => *total += d,
            (Some(egui::Event::MouseWheel { delta: total, .. }), egui::Event::MouseWheel { delta, .. }) => *total += delta,
            (_, e) => merged.push(e),
        }
    }
    *events = merged;
}

/// The pointer moving or the wheel turning (not a touchpad's scroll starting or ending).
fn is_move(e: &egui::Event) -> bool {
    matches!(e, egui::Event::PointerMoved(_) | egui::Event::MouseMoved(_) | egui::Event::MouseWheel { phase: egui::TouchPhase::Move, .. })
}

/// Whether [`merge_moves`] adds `e` to `m`: the same kind of move, and for the wheel the same unit
/// and modifiers (Ctrl turns zoom).
fn merges_with(m: &egui::Event, e: &egui::Event) -> bool {
    match (m, e) {
        (egui::Event::PointerMoved(_), egui::Event::PointerMoved(_)) | (egui::Event::MouseMoved(_), egui::Event::MouseMoved(_)) => true,
        (egui::Event::MouseWheel { unit: u, modifiers: mods, .. }, egui::Event::MouseWheel { unit, modifiers, .. }) => u == unit && mods == modifiers,
        _ => false,
    }
}

/// An event that may open or close a window, which ends its frame's input ([`pace_input`]): a
/// click (a button coming up), a touch ending, a screen reader's action, the clipboard's events
/// (Mod+Alt+V arrives as a paste and opens Paste Special), a key going down but plain typing on
/// the page (`typing`: no Ctrl, Alt or Command and no function key; Shift is typing), and the one
/// key the app reads as it comes up, V (`keys::canvas_events`: Mod+Alt+V with a picture on the
/// clipboard opens Paste Special then, also once Mod has come up first). Never a modifier key
/// alone, a held key's repeat ([`mark_repeats`]), the pointer or a touch moving, leaving or going
/// down, the wheel and gestures, typed text and composition, the window's focus, a screenshot.
/// Every kind of event is named, so a new one gets a rule.
fn may_open(e: &egui::Event, typing: bool) -> bool {
    use egui::{Event, Key};
    match e {
        Event::PointerMoved(_)
        | Event::MouseMoved(_)
        | Event::PointerButton { pressed: true, .. }
        | Event::PointerGone
        | Event::MouseWheel { .. }
        | Event::Zoom(_)
        | Event::Rotate(_)
        | Event::Text(_)
        | Event::Ime(_)
        | Event::ModifiersChanged(_)
        | Event::WindowFocused(_)
        | Event::Screenshot { .. }
        | Event::Touch { phase: egui::TouchPhase::Start | egui::TouchPhase::Move, .. } => false,
        Event::Key { key, pressed, repeat, modifiers, .. } => {
            let modifier = matches!(
                key,
                Key::ShiftLeft
                    | Key::ShiftRight
                    | Key::ControlLeft
                    | Key::ControlRight
                    | Key::AltLeft
                    | Key::AltRight
                    | Key::SuperLeft
                    | Key::SuperRight
            );
            // F1 to F35.
            let function = key.name().strip_prefix('F').is_some_and(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()));
            let chord = modifiers.ctrl || modifiers.alt || modifiers.command || modifiers.mac_cmd;
            if modifier || (*pressed && *repeat) {
                false
            } else if *pressed {
                !typing || chord || function
            } else {
                matches!(key, Key::V | Key::Paste)
            }
        }
        Event::PointerButton { pressed: false, .. }
        | Event::Touch { phase: egui::TouchPhase::End | egui::TouchPhase::Cancel, .. }
        | Event::Paste(_)
        | Event::Copy
        | Event::Cut
        | Event::AccessKitActionRequest(_) => true,
    }
}

/// Whether as many modal windows are open now as the last pass drew a backdrop under, ready to
/// take clicks ([`backdrop`]); also with none open. Until then [`pace_input`] holds the input.
/// One window can take another's place in a pass (Paragraph's Tabs button): the backdrop of its
/// level is up, so the new window's first click lands on it rather than behind.
pub fn settled(app: &WordApp, ctx: &egui::Context) -> bool {
    !open(app) || covered_last_pass(ctx).is_some_and(|covered| covered.len() == levels(app))
}

/// How many modal windows are open, one over another: a modal dialog, the built-in file browser,
/// the Colors dialog, Zotero's alert. [`backdrop`] covers as many.
pub(crate) fn levels(app: &WordApp) -> usize {
    let dialog = app.dialog.as_ref().is_some_and(|d| !modeless(d));
    usize::from(dialog) + usize::from(app.file_browser_open()) + usize::from(app.colors.is_some()) + usize::from(app.zotero.alert_open())
}

fn is_pointer(e: &egui::Event) -> bool {
    matches!(
        e,
        egui::Event::PointerMoved(_)
            | egui::Event::MouseMoved(_)
            | egui::Event::PointerButton { .. }
            | egui::Event::PointerGone
            | egui::Event::Zoom(_)
            | egui::Event::Rotate(_)
            | egui::Event::Touch { .. }
            | egui::Event::MouseWheel { .. }
            | egui::Event::AccessKitActionRequest(_)
    )
}

/// A button was down as a modal dialog opened ([`backdrop`]): the press behind the dialog ends
/// without a click, as a system dialog takes the mouse from the window. The pointer leaves for a
/// frame (`Event::PointerGone`, on which egui drops the click and drag it was waiting for), and
/// this frame's input from its first pointer event (or screen reader's action) on waits for the
/// next, the button's release with it.
pub fn end_press(events: &mut Vec<egui::Event>, held: &mut Vec<egui::Event>) {
    let first = events.iter().position(is_pointer).unwrap_or(events.len());
    let mut later = events.split_off(first);
    later.append(held);
    *held = later;
    events.push(egui::Event::PointerGone);
}

/// Input that reaches a widget without the pointer over it: keys, text, composition, the
/// clipboard, the modifiers changing (an Alt tap arms KeyTips), and a screen reader's actions,
/// which name the widget they click or set.
fn reaches_without_pointer(e: &egui::Event) -> bool {
    matches!(
        e,
        egui::Event::Key { .. }
            | egui::Event::Text(_)
            | egui::Event::Ime(_)
            | egui::Event::Paste(_)
            | egui::Event::Copy
            | egui::Event::Cut
            | egui::Event::ModifiersChanged(_)
            | egui::Event::AccessKitActionRequest(_)
    )
}

/// Files dropped on the window open documents, and its close button asks about unsaved changes,
/// both read before the frame's other input and able to bring up a window (Save Changes): that
/// input waits for the next frame, which finds it open ([`pace_input`]'s rule).
pub fn hold_all(events: &mut Vec<egui::Event>, held: &mut Vec<egui::Event>) {
    let mut later = std::mem::take(events);
    later.append(held);
    *held = later;
}

/// The keyboard's side of [`backdrop`]: while a modal window is open, the window behind it takes no
/// key and no screen reader's action either, also where it reads keys itself (Draw Table's Escape,
/// the File page's Escape, KeyTips, SmartArt's Tab). They are taken out of the input as the pass
/// starts (`WordApp::logic`: with [`pace_input`] and `WordApp::outside_input` no modal window opens
/// before the input is read), and given back with [`keys_back`] just before the [`top`] window is
/// drawn (if one closes before, as a document replaced by recovery closes its dialog, they are
/// dropped). Nothing is
/// taken without a modal window.
pub fn keys_away(app: &WordApp, ctx: &egui::Context) -> Vec<egui::Event> {
    if !open(app) {
        return Vec::new();
    }
    ctx.input_mut(|i| {
        let (keys, rest) = std::mem::take(&mut i.events).into_iter().partition(reaches_without_pointer);
        i.events = rest;
        keys
    })
}

/// Gives back the keys [`keys_away`] took, as the `window` about to be drawn is the [`top`] one.
pub fn keys_back(ctx: &egui::Context, keys: &mut Vec<egui::Event>, top: Top, window: Top) {
    if top == window && !keys.is_empty() {
        let keys = std::mem::take(keys);
        ctx.input_mut(|i| i.events.extend(keys));
    }
}

fn stack_id() -> egui::Id {
    egui::Id::new("wordcraft.modalStack")
}

fn covered_id() -> egui::Id {
    egui::Id::new("wordcraft.modalCovered")
}

/// The dialog `d` is shown on `layer` this pass. A modal one is a window [`backdrop`] goes under;
/// a modeless one that took the keyboard from the page keeps it until a press elsewhere (the
/// page, the ribbon), as a click elsewhere takes the keyboard from a window
/// ([`dialog_has_keyboard`]).
pub fn shown(ctx: &egui::Context, d: &Dialog, layer: egui::LayerId) {
    if modeless(d) {
        // On the page, the ribbon or a pane: not the dialog's own lists, over everything.
        let pressed = ctx.input(|i| if i.pointer.any_pressed() { i.pointer.press_origin() } else { None });
        if pressed.and_then(|p| ctx.layer_id_at(p)).is_some_and(|l| l != layer && l.order <= egui::Order::Middle) {
            ctx.data_mut(|data| data.remove::<bool>(dialog_keys_id()));
        }
    } else {
        let pass = ctx.cumulative_pass_nr();
        ctx.data_mut(|data| data.insert_temp(stack_id(), (vec![layer], pass)));
    }
}

/// A modal window over whatever is open (the file browser, the Colors dialog, Zotero's alert) is
/// shown on `layer`: it has a backdrop of its own, over the windows below it, so they take no click
/// either (the Colors dialog's over its own `egui::Modal` one, which closes it on a click).
pub fn shown_on_top(ctx: &egui::Context, layer: egui::LayerId) {
    let mut stack = stack_this_pass(ctx);
    stack.push(layer);
    let pass = ctx.cumulative_pass_nr();
    ctx.data_mut(|data| data.insert_temp(stack_id(), (stack, pass)));
}

/// The modal windows shown this pass, bottom first: the dialog, and what is open above it.
fn stack_this_pass(ctx: &egui::Context) -> Vec<egui::LayerId> {
    let pass = ctx.cumulative_pass_nr();
    ctx.data(|data| data.get_temp::<(Vec<egui::LayerId>, u64)>(stack_id())).filter(|(_, p)| *p == pass).map(|(stack, _)| stack).unwrap_or_default()
}

/// The modal windows [`backdrop`] covered the window with in the last pass, if it did.
fn covered_last_pass(ctx: &egui::Context) -> Option<Vec<egui::LayerId>> {
    let pass = ctx.cumulative_pass_nr();
    ctx.data(|data| data.get_temp::<(Vec<egui::LayerId>, u64)>(covered_id())).filter(|(_, p)| p.checked_add(1) == Some(pass)).map(|(stack, _)| stack)
}

/// Whether `layer` is behind a modal window: the modal windows covered the window in the last pass,
/// and `layer` isn't the top one. A drop-down of a window below (the ribbon's collapsed groups, the
/// font list, a dialog's list under the Colors dialog, drawn over everything) closes then, as menus
/// close as a modal window opens.
pub fn behind(ctx: &egui::Context, layer: egui::LayerId) -> bool {
    covered_last_pass(ctx).is_some_and(|stack| stack.last() != Some(&layer))
}

/// Runs every pass, after every window. While a modal dialog is open nothing behind it takes a
/// click, drag or hover: an invisible backdrop over the whole window lies directly under the
/// dialog (a sublayer, which egui keeps right above its parent, also once a click on the
/// backdrop raises it), above everything else in the window, the menus and lists over the page
/// included, as `egui::Modal` has it. A modal window over another (the file browser, the Colors
/// dialog, Zotero's alert) has a backdrop of its own, over the one below: each comes up on top as
/// it first shows. Tab stays within the top window.
///
/// The windows count as covered ([`settled`]) once each backdrop takes clicks: a layer egui shows
/// for the first time is measured first and takes no click in the next frame, so that frame's
/// input waits one more frame ([`pace_input`]).
///
/// As a modal window opens, what goes on behind it ends, as when a system dialog disables the
/// window: a drag (also egui's own, so it doesn't go on once the dialog closes with the button
/// still down), a button held down ([`end_press`]), the page's gestures, its menus and lists, and
/// a field's hold on the keyboard. Also for a dialog that has closed in the pass it opened in (its
/// shortcut and Enter together).
pub fn backdrop(app: &mut WordApp, ctx: &egui::Context) {
    let stack = stack_this_pass(ctx);
    let is_open = open(app);
    if !is_open && stack.is_empty() {
        return;
    }
    if covered_last_pass(ctx).is_none_or(|covered| covered.len() < levels(app)) {
        end_behind(app, ctx, stack.last().copied());
    }
    // Closed in this pass, or opened after the windows were drawn: the next pass covers it.
    let Some(&top) = stack.last() else { return };
    if !is_open {
        return;
    }
    let pass = ctx.cumulative_pass_nr();
    // The whole window, the system's insets at its edges included; nothing for a screen reader.
    let rect = ctx.viewport_rect();
    let mut takes_clicks = true;
    for (level, window) in stack.iter().enumerate() {
        let id = egui::Id::new(("wordcraft.modalBackdrop", level));
        takes_clicks &= egui::AreaState::load(ctx, id).is_some();
        let shown = egui::Area::new(id).order(egui::Order::Foreground).fixed_pos(rect.min).movable(false).interactable(true).show(ctx, |ui| {
            let _backdrop = ui.allocate_response(rect.size(), egui::Sense::CLICK | egui::Sense::DRAG);
        });
        ctx.set_sublayer(shown.response.layer_id, *window);
    }
    ctx.memory_mut(|m| m.set_modal_layer(top));
    if takes_clicks {
        ctx.data_mut(|data| data.insert_temp(covered_id(), (stack, pass)));
    }
}

/// A modal dialog has opened on `top` (or opened after the windows were drawn, or from outside the
/// window, and shows next pass): the drags, gestures, menus, lists and KeyTips behind it end, a button held down ends without a
/// click, and a field behind it (a box on the ribbon or in a pane) gives up the keyboard. Nothing
/// of the dialog is dragged yet: it wasn't shown before.
pub(crate) fn end_behind(app: &mut WordApp, ctx: &egui::Context, top: Option<egui::LayerId>) {
    ctx.stop_dragging();
    app.canvas.cancel_pointer_gestures();
    app.canvas.context_menu_open = false;
    crate::content_controls::close_popup(app);
    egui::Popup::close_all(ctx);
    crate::keytips::end(app, ctx);
    if ctx.input(|i| i.pointer.any_down()) {
        app.end_press = true;
    }
    // Only a field that had the keyboard before this pass: a field of the new window asking for it
    // now is left alone, and isn't read either (reading a response settles a click elsewhere in
    // this pass on it, which would take the keyboard from it).
    if let Some(field) = ctx.memory(|m| m.focused())
        && ctx.memory(|m| m.had_focus_last_frame(field))
        && ctx.read_response(field).is_some_and(|r| Some(r.layer_id) != top)
    {
        // The page gets it back as the window closes, as when [`keyboard`] takes it.
        if app.canvas.widget_id.is_some_and(|(page, _)| page == field) {
            app.canvas.focused = false;
            ctx.data_mut(|d| d.insert_temp(took_focus_id(), true));
        }
        ctx.memory_mut(|m| m.surrender_focus(field));
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

    /// Frames as the window runs them: the input goes through `WordApp::raw_input_hook` first, so
    /// input after an event that may open a window waits for the next frame (`pace_input`). Only
    /// input put in `input_mut().events` goes through it (`one_frame`): `Harness::event` gives each
    /// event a frame of its own past the hook, so those tests prove what holds in every frame
    /// (`keys_away`, the backdrop), not the pacing.
    trait Frames {
        fn frame(&mut self);
        fn frames(&mut self, n: usize);
        /// Frames until no input waits.
        fn settle(&mut self);
    }

    impl Frames for Harness<'static, WordApp> {
        fn frame(&mut self) {
            let mut raw = egui::RawInput { events: std::mem::take(&mut self.input_mut().events), ..Default::default() };
            self.state_mut().raw_input_hook(&mut raw);
            self.input_mut().events = raw.events;
            self.step();
        }

        fn frames(&mut self, n: usize) {
            for _ in 0..n {
                self.frame();
            }
        }

        fn settle(&mut self) {
            for _ in 0..20 {
                if self.state().held_input.is_empty() {
                    return;
                }
                self.frame();
            }
            panic!("input still waits after 20 frames");
        }
    }

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
        h.frames(6);
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
        h.frames(2);
    }

    /// A letter as the keyboard sends it: the key, then its text.
    fn type_letter(h: &mut Harness<'static, WordApp>, key: Key, text: &str) {
        let [down, up] = key_events(key, Modifiers::NONE);
        h.event(down);
        h.event(Event::Text(text.into()));
        h.event(up);
        h.frames(2);
    }

    fn mod_key() -> Modifiers {
        if cfg!(target_os = "macos") { Modifiers::MAC_CMD | Modifiers::COMMAND } else { Modifiers::CTRL | Modifiers::COMMAND }
    }

    fn body(h: &Harness<'static, WordApp>) -> String {
        h.state().session.doc.plain_text(StoryRef::Body)
    }

    fn click(h: &mut Harness<'static, WordApp>, p: egui::Pos2) {
        h.event(Event::PointerMoved(p));
        h.frame();
        let button = |pressed| Event::PointerButton { pos: p, button: egui::PointerButton::Primary, pressed, modifiers: Modifiers::NONE };
        h.event(button(true));
        h.frame();
        h.event(button(false));
        h.frames(2);
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
        h.frames(2);
        assert!(h.state().dialog.is_none(), "Enter is the dialog's OK");
        assert_eq!(body(&h), TEXT, "neither the letter nor Enter reach the document");

        for e in key_events(Key::D, mod_key()) {
            h.event(e);
        }
        h.frame();
        type_letter(&mut h, Key::X, "x");
        assert!(h.state().dialog.is_some());
        assert_eq!(body(&h), TEXT, "a letter in the next frame doesn't either");
    }

    /// Keys that arrive together, in one frame (`Harness::event` gives each event a frame of its
    /// own), then the frames for what waited and for what they opened.
    fn one_frame(h: &mut Harness<'static, WordApp>, events: impl IntoIterator<Item = Event>) {
        h.input_mut().events.extend(events);
        h.frame();
        h.settle();
        h.frames(2);
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
        h.frame();
        assert!(matches!(h.state().dialog, Some(Dialog::Find { .. })), "Mod+H opens Find and Replace");
        one_frame(&mut h, letter(Key::X, "x"));
        assert_eq!(body(&h), TEXT, "the letter isn't typed");
        press(&mut h, Key::Escape, Modifiers::NONE);
        assert!(h.state().dialog.is_none(), "Escape closes Find");

        h.input_mut().events.extend(key_events(Key::H, mod_key()));
        h.frame();
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
        h.frame();
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
        h.frame();
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
        h.frames(2);
        h.input_mut().events.extend(key_events(Key::Enter, Modifiers::NONE));
        h.frame();
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
        h.frames(2);
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
        h.frames(2);
        assert!(h.state().canvas.focused);
        let depth = h.state().session.undo_depth();
        let alt_shift = Modifiers { alt: true, shift: true, ..Modifiers::NONE };
        one_frame(&mut h, key_events(Key::X, alt_shift).into_iter().chain(key_events(Key::B, mod_key())));
        assert!(matches!(h.state().dialog, Some(Dialog::Refs(_))), "Alt+Shift+X opens Mark Index Entry");
        assert_eq!(h.state().session.undo_depth(), depth, "Mod+B formatted nothing");
    }

    /// A modeless dialog keeps the keyboard it took from the page until a press elsewhere: after a
    /// click on Bold on the ribbon, with Find still open, Mod+B is the document's shortcut again.
    #[test]
    fn a_click_elsewhere_ends_a_modeless_dialogs_hold_on_the_keyboard() {
        use egui_kittest::kittest::Queryable;
        let mut h = harness();
        h.state_mut().run("select.all", json!({})).unwrap();
        h.frames(2);
        one_frame(&mut h, key_events(Key::H, mod_key()));
        assert!(matches!(h.state().dialog, Some(Dialog::Find { .. })), "Mod+H opens Find and Replace");
        assert!(super::dialog_has_keyboard(h.state(), &h.ctx), "Find has the keyboard");
        let depth = h.state().session.undo_depth();
        let bold = h.query_all_by_label("Bold").map(|n| n.rect()).min_by(|a, b| a.min.y.total_cmp(&b.min.y)).expect("Bold on the ribbon");
        click(&mut h, bold.center());
        assert_eq!(h.state().session.undo_depth(), depth + 1, "the click applies Bold");
        assert!(!super::dialog_has_keyboard(h.state(), &h.ctx), "the click ends Find's hold");
        // Nothing holds the keyboard (Find's box gave it up), as after a click on the ribbon.
        h.ctx.memory_mut(|m| {
            if let Some(f) = m.focused() {
                m.surrender_focus(f);
            }
        });
        h.frames(2);
        one_frame(&mut h, key_events(Key::B, mod_key()));
        assert_eq!(h.state().session.undo_depth(), depth + 2, "Mod+B is the document's shortcut");
    }

    /// Go To's page box takes the keyboard as the tab opens, but a click on the page gives the
    /// page the keyboard with the dialog still open: the box doesn't take it back.
    #[test]
    fn the_go_to_box_leaves_the_keyboard_to_the_page() {
        let mut h = harness();
        h.state_mut().run("ui.dialog", json!({"name": "goto"})).unwrap();
        h.frames(3);
        assert!(!h.state().canvas.focused, "Go To takes the keyboard");
        type_letter(&mut h, Key::Num2, "2");
        assert!(matches!(&h.state().dialog, Some(Dialog::Find { goto_target, .. }) if goto_target == "2"), "the digit is in the box");
        let p = on_the_page(&h);
        let dialog = h.ctx.memory(|m| m.area_rect(egui::Id::new(("dialog", "Find and Replace")))).expect("the dialog is shown");
        assert!(!dialog.contains(p), "{p:?} is outside the dialog {dialog:?}");
        click(&mut h, p);
        h.frames(2);
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
        h.frames(3);
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
        h.frames(2);
        let depth = h.state().session.undo_depth();
        press(&mut h, Key::B, mod_key());
        assert_eq!(h.state().session.undo_depth(), depth + 1, "without a dialog, Mod+B formats");
        h.state_mut().run("edit.undo", json!({})).unwrap();

        h.state_mut().run("ui.dialog", json!({"name": "paragraph"})).unwrap();
        h.frames(2);
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
        h.frames(2);
        let start = format!("{:?}", h.state().session.sel);
        let p = on_the_page(&h);
        click(&mut h, p);
        assert_ne!(format!("{:?}", h.state().session.sel), start, "without a dialog the click moves the caret");

        h.state_mut().run("caret.docStart", json!({})).unwrap();
        h.state_mut().run("ui.dialog", json!({"name": "paragraph"})).unwrap();
        h.frames(3);
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
            h.frames(2);
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
        h.frames(3);
        let strip = h.state().canvas.eq_options.expect("the equation's frame is shown").strip.center();
        h.event(Event::PointerMoved(strip));
        h.frames(2);
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
        h.frame();
        h.event(button(from, true));
        h.frame();
        h.event(Event::PointerMoved(to));
        h.frame();
        one_frame(h, before_release);
        h.event(button(to, false));
        h.frames(2);
    }

    /// A drag on the page still going when a modal dialog opens (its shortcut pressed with the
    /// button down) is dropped, as Escape drops it: releasing the button over the dialog and
    /// closing the dialog draw nothing. Without the dialog the same drag draws a table.
    #[test]
    fn a_drag_on_the_page_ends_when_a_modal_dialog_opens() {
        let mut h = harness();
        h.state_mut().run("table.draw", json!({"on": true})).unwrap();
        h.frames(2);
        let to = on_the_page(&h);
        let from = to - egui::vec2(200.0, 150.0);
        let depth = h.state().session.undo_depth();
        draw_stroke(&mut h, from, to, []);
        assert_eq!(h.state().session.undo_depth(), depth + 1, "without a dialog the drag draws a table");
        h.state_mut().run("edit.undo", json!({})).unwrap();
        h.frames(2);

        let depth = h.state().session.undo_depth();
        draw_stroke(&mut h, from, to, key_events(Key::D, mod_key()));
        assert!(matches!(h.state().dialog, Some(Dialog::Font { .. })), "Mod+D opens Font");
        press(&mut h, Key::Escape, Modifiers::NONE);
        assert!(h.state().dialog.is_none(), "Escape cancels");
        h.frames(2);
        assert_eq!(h.state().session.undo_depth(), depth, "no table is drawn");
        assert!(h.state().canvas.table_stroke.is_none(), "the stroke is dropped");
    }

    /// The screen point over page point (`x`, `y`) of the first page.
    fn on_page_point(h: &mut Harness<'static, WordApp>, x: f32, y: f32) -> egui::Pos2 {
        crate::canvas::page_to_screen(h.state_mut(), 0, x, y).expect("the first page is on screen")
    }

    /// A drag from `from` to `to`, with `halfway` in a frame of its own halfway there, before the
    /// pointer goes on to `to` and the button comes up.
    fn drag_with(h: &mut Harness<'static, WordApp>, from: egui::Pos2, to: egui::Pos2, halfway: impl IntoIterator<Item = Event>) {
        let button = |pos, pressed| Event::PointerButton { pos, button: egui::PointerButton::Primary, pressed, modifiers: Modifiers::NONE };
        h.event(Event::PointerMoved(from));
        h.frame();
        h.event(button(from, true));
        h.frame();
        h.event(Event::PointerMoved(from + (to - from) / 2.0));
        h.frame();
        one_frame(h, halfway);
        h.event(Event::PointerMoved(to));
        h.frame();
        h.event(button(to, false));
        h.frames(2);
    }

    /// A drag on a ruler still going when a modal dialog opens (its shortcut pressed with the
    /// button down) ends there: the top margin stays where the drag had taken it, and a tab stop
    /// being placed isn't set where the button comes up, over the dialog. A click on the ruler in
    /// the frame the dialog opens in sets no stop either. Without the dialog the same drags move
    /// the margin and set the stop.
    #[test]
    fn a_drag_on_a_ruler_ends_when_a_modal_dialog_opens() {
        let mut h = harness();
        h.state_mut().run("view.ruler", json!({"value": true})).unwrap();
        h.frames(4);
        let body = h.state_mut().session.layout().pages[0].body;
        let area = h.state().canvas.canvas_rect.expect("the canvas is shown");
        let top = |h: &Harness<'static, WordApp>| wordcraft_engine::cmd::page::sect(&h.state().session).margin_top;
        let stops = |h: &Harness<'static, WordApp>| {
            let s = &h.state().session;
            let para = s.doc.para_at(&s.sel.focus).expect("a paragraph");
            s.doc.styles.resolve_para(&para.props).tabs.len()
        };
        // The top margin's edge on the vertical ruler, dragged 60 points down.
        let on_vertical = |p: egui::Pos2| egui::pos2(area.min.x - 11.0, p.y);
        let from = on_vertical(on_page_point(&mut h, 0.0, body.y));
        let to = on_vertical(on_page_point(&mut h, 0.0, body.y + 60.0));
        let start = top(&h);
        drag_with(&mut h, from, to, []);
        assert!(top(&h) > start + 50.0, "without a dialog the drag moves the margin: {start} → {}", top(&h));
        h.state_mut().run("edit.undo", json!({})).unwrap();
        h.frames(2);
        assert_eq!(top(&h), start);
        drag_with(&mut h, from, to, key_events(Key::D, mod_key()));
        assert!(matches!(h.state().dialog, Some(Dialog::Font { .. })), "Mod+D opens Font");
        press(&mut h, Key::Escape, Modifiers::NONE);
        assert!(h.state().dialog.is_none(), "Escape cancels");
        let halfway = top(&h);
        assert!(halfway < start + 45.0, "the margin stays where the drag was as Font opened: {start} → {halfway}");
        // Font opened and closed (Mod+D and Enter, its OK, together) with the button still down:
        // the drag doesn't go on once Font has closed.
        h.state_mut().run("edit.undo", json!({})).unwrap();
        h.frames(2);
        assert_eq!(top(&h), start);
        drag_with(&mut h, from, to, key_events(Key::D, mod_key()).into_iter().chain(key_events(Key::Enter, Modifiers::NONE)));
        assert!(h.state().dialog.is_none(), "Enter is Font's OK");
        assert!(top(&h) < start + 45.0, "the drag ended as Font opened: {start} → {}", top(&h));

        // On the horizontal ruler, over the text: a stop placed with a drag from 1 to 2 inches.
        let on_horizontal = |p: egui::Pos2| egui::pos2(p.x, area.min.y - 7.0);
        let from = on_horizontal(on_page_point(&mut h, body.x + 72.0, 0.0));
        let to = on_horizontal(on_page_point(&mut h, body.x + 144.0, 0.0));
        assert_eq!(stops(&h), 0);
        drag_with(&mut h, from, to, []);
        assert_eq!(stops(&h), 1, "without a dialog the drag sets a stop");
        h.state_mut().run("edit.undo", json!({})).unwrap();
        h.frames(2);
        assert_eq!(stops(&h), 0);
        drag_with(&mut h, from, to, key_events(Key::D, mod_key()));
        assert!(matches!(h.state().dialog, Some(Dialog::Font { .. })), "Mod+D opens Font");
        press(&mut h, Key::Escape, Modifiers::NONE);
        assert_eq!(stops(&h), 0, "no stop is set over the dialog");

        // A click on the ruler that comes with the shortcut.
        let at = on_horizontal(on_page_point(&mut h, body.x + 216.0, 0.0));
        h.event(Event::PointerMoved(at));
        h.frames(2);
        let button = |pressed| Event::PointerButton { pos: at, button: egui::PointerButton::Primary, pressed, modifiers: Modifiers::NONE };
        one_frame(&mut h, key_events(Key::D, mod_key()).into_iter().chain([button(true), button(false)]));
        assert!(matches!(h.state().dialog, Some(Dialog::Font { .. })), "Mod+D opens Font");
        assert_eq!(stops(&h), 0, "the click sets no stop behind the dialog");
    }

    /// A click on the page in the frame a modal dialog opens in leaves the selected comment
    /// balloon selected (its drafts aren't written behind the dialog); once the dialog has
    /// closed, the same click leaves the balloon.
    #[test]
    fn a_click_in_the_frame_a_modal_dialog_opens_in_keeps_the_balloon() {
        let mut h = harness();
        h.state_mut().run("select.text", json!({"text": "kota"})).unwrap();
        let id = h.state_mut().run("review.newComment", json!({"text": "Note"})).unwrap()["id"].as_u64().unwrap() as u32;
        h.state_mut().run("view.commentsPane", json!({"value": false})).unwrap();
        h.frames(4);
        h.state_mut().canvas.balloon = Some(id);
        let p = on_the_page(&h);
        h.event(Event::PointerMoved(p));
        h.frames(2);
        let button = |pressed| Event::PointerButton { pos: p, button: egui::PointerButton::Primary, pressed, modifiers: Modifiers::NONE };
        one_frame(&mut h, key_events(Key::D, mod_key()).into_iter().chain([button(true), button(false)]));
        assert!(matches!(h.state().dialog, Some(Dialog::Font { .. })), "Mod+D opens Font");
        assert_eq!(h.state().canvas.balloon, Some(id), "the balloon stays selected");
        press(&mut h, Key::Escape, Modifiers::NONE);
        assert!(h.state().dialog.is_none(), "Escape cancels");
        assert_eq!(h.state().canvas.balloon, Some(id));
        click(&mut h, p);
        assert_eq!(h.state().canvas.balloon, None, "a click on the page leaves the balloon");
    }

    /// How a modal dialog (Font) comes up in a frame.
    #[derive(Clone, Copy, Debug)]
    enum Opener {
        /// Its shortcut, with the page holding the keyboard: the page reads it.
        ShortcutOnThePage,
        /// Its shortcut, with the keyboard nowhere: the window's shortcuts read it, after the page
        /// and the ribbon have been drawn.
        ShortcutElsewhere,
        /// An agent, before the frame: nothing has been drawn with the dialog yet.
        Agent,
    }

    /// What the window behind a dialog shows of a click: the caret, the page's menu, the caret
    /// paragraph's tab stops, the selected comment balloon and the undo depth.
    fn behind(h: &Harness<'static, WordApp>) -> String {
        let s = &h.state().session;
        let stops = s.doc.para_at(&s.sel.focus).map_or(0, |para| s.doc.styles.resolve_para(&para.props).tabs.len());
        let c = &h.state().canvas;
        format!("{:?} menu {} stops {stops} balloon {:?} undo {}", s.sel, c.context_menu_open, c.balloon, s.undo_depth())
    }

    /// Back to the start of a case: no dialog or menu, the undo depth `depth`, "Ala" selected, no
    /// balloon, and the page with the keyboard.
    fn reset_behind(h: &mut Harness<'static, WordApp>, depth: usize) {
        press(h, Key::Escape, Modifiers::NONE);
        while h.state().session.undo_depth() > depth {
            h.state_mut().run("edit.undo", json!({})).unwrap();
        }
        h.state_mut().run("select.text", json!({"text": "Ala"})).unwrap();
        h.state_mut().canvas.balloon = None;
        h.state_mut().canvas.want_focus = true;
        h.frames(3);
    }

    /// Nothing behind a modal dialog takes the click that comes in the frame the dialog comes up
    /// in, however it comes up: not the page (the caret stays, no menu), its ruler (no tab stop),
    /// a comment balloon (not selected) or the ribbon (Bold doesn't apply). Each click does its
    /// part without the dialog; and one that comes before the shortcut in its frame still does.
    #[test]
    fn nothing_behind_a_modal_dialog_takes_the_click_that_comes_as_it_opens() {
        use egui_kittest::kittest::Queryable;
        let mut h = harness();
        h.state_mut().run("view.ruler", json!({"value": true})).unwrap();
        h.state_mut().run("select.text", json!({"text": "kota"})).unwrap();
        let id = h.state_mut().run("review.newComment", json!({"text": "Note"})).unwrap()["id"].as_u64().unwrap() as u32;
        h.state_mut().run("view.commentsPane", json!({"value": false})).unwrap();
        h.frames(4);
        let body = h.state_mut().session.layout().pages[0].body;
        let area = h.state().canvas.canvas_rect.expect("the canvas is shown");
        let card = h.state().canvas.balloon_rects.iter().find(|(i, _)| *i == id).map(|(_, r)| *r).expect("the comment has a balloon");
        let bold = h.query_all_by_label("Bold").map(|n| n.rect()).min_by(|a, b| a.min.y.total_cmp(&b.min.y)).expect("Bold on the ribbon");
        let ruler = egui::pos2(on_page_point(&mut h, body.x + 216.0, 0.0).x, area.min.y - 7.0);
        let page = on_the_page(&h);
        let targets = [
            ("a click on the page", page, egui::PointerButton::Primary),
            ("a right-click on the page", page, egui::PointerButton::Secondary),
            ("a click on the ruler", ruler, egui::PointerButton::Primary),
            ("a click on the balloon", card.left_top() + egui::vec2(30.0, 8.0), egui::PointerButton::Primary),
            ("a click on Bold", bold.center(), egui::PointerButton::Primary),
        ];
        let depth = h.state().session.undo_depth();
        let reset = |h: &mut Harness<'static, WordApp>| reset_behind(h, depth);
        let clicked = |at: egui::Pos2, button: egui::PointerButton| {
            let e = |pressed| Event::PointerButton { pos: at, button, pressed, modifiers: Modifiers::NONE };
            [Event::PointerMoved(at), e(true), e(false)]
        };
        for (what, at, button) in targets {
            reset(&mut h);
            let before = behind(&h);
            one_frame(&mut h, clicked(at, button));
            assert_ne!(behind(&h), before, "{what}: without a dialog it does something");
            for opener in [Opener::ShortcutOnThePage, Opener::ShortcutElsewhere, Opener::Agent] {
                reset(&mut h);
                let mut events: Vec<Event> = Vec::new();
                match opener {
                    Opener::ShortcutOnThePage => {
                        assert!(h.state().canvas.focused, "{what}, {opener:?}: the page has the keyboard");
                        events.extend(key_events(Key::D, mod_key()));
                    }
                    Opener::ShortcutElsewhere => {
                        let (page_id, _) = h.state().canvas.widget_id.expect("the page is drawn");
                        h.ctx.memory_mut(|m| m.surrender_focus(page_id));
                        h.state_mut().canvas.focused = false;
                        h.frames(2);
                        assert!(!h.state().canvas.focused && h.ctx.memory(|m| m.focused()).is_none(), "{what}, {opener:?}: no keyboard");
                        events.extend(key_events(Key::D, mod_key()));
                    }
                    Opener::Agent => {
                        h.state_mut().run("ui.dialog", json!({"name": "font"})).unwrap();
                    }
                }
                let before = behind(&h);
                one_frame(&mut h, events.into_iter().chain(clicked(at, button)));
                assert!(matches!(h.state().dialog, Some(Dialog::Font { .. })), "{what}, {opener:?}: Font opens");
                assert_eq!(behind(&h), before, "{what}, {opener:?}: nothing behind the dialog takes the click");
            }
        }
        // A click before the shortcut, in the same frame, comes first: it moves the caret.
        reset(&mut h);
        let before = behind(&h);
        one_frame(&mut h, clicked(page, egui::PointerButton::Primary).into_iter().chain(key_events(Key::D, mod_key())));
        assert!(matches!(h.state().dialog, Some(Dialog::Font { .. })), "Font opens");
        assert_ne!(behind(&h), before, "the click before the shortcut moves the caret");
    }

    /// A modal dialog is over everything else in the window, its backdrop right under it, also
    /// once a click on the backdrop has raised the backdrop. A file browser opened from the dialog
    /// is over the dialog, with a backdrop of its own between them. As the dialog opens, the
    /// page's menu closes.
    #[test]
    fn a_modal_dialog_and_its_backdrop_are_over_everything_else() {
        let mut h = harness();
        let p = on_the_page(&h);
        let right = |pressed| Event::PointerButton { pos: p, button: egui::PointerButton::Secondary, pressed, modifiers: Modifiers::NONE };
        h.event(Event::PointerMoved(p));
        one_frame(&mut h, [right(true), right(false)]);
        assert!(h.state().canvas.context_menu_open, "a right-click opens the page's menu");
        press(&mut h, Key::D, mod_key());
        assert!(matches!(h.state().dialog, Some(Dialog::Font { .. })), "Mod+D opens Font");
        assert!(!h.state().canvas.context_menu_open, "the menu closes as Font opens");
        let backdrop = |level: usize| egui::LayerId::new(egui::Order::Foreground, egui::Id::new(("wordcraft.modalBackdrop", level)));
        let dialog = egui::LayerId::new(egui::Order::Foreground, egui::Id::new(("dialog", "Font")));
        // The layers shown from the backdrop up, but for the tooltips and the like over everything.
        let from = |h: &Harness<'static, WordApp>, layer: egui::LayerId| {
            let order: Vec<egui::LayerId> = h.ctx.memory(|m| m.layer_ids().filter(|l| m.areas().is_visible(l)).collect());
            let at = order.iter().position(|l| *l == layer).expect("the backdrop is shown");
            order.into_iter().skip(at).filter(|l| l.order <= egui::Order::Foreground).collect::<Vec<_>>()
        };
        assert_eq!(from(&h, backdrop(0)), [backdrop(0), dialog], "Font is right over its backdrop, over everything else");
        // egui names the top modal window for a point on a layer under it: the backdrop's.
        assert_eq!(h.ctx.layer_id_at(p), Some(dialog), "what is meant for the page goes to Font");
        let left = |pressed| Event::PointerButton { pos: p, button: egui::PointerButton::Primary, pressed, modifiers: Modifiers::NONE };
        one_frame(&mut h, [left(true), left(false)]);
        assert_eq!(from(&h, backdrop(0)), [backdrop(0), dialog], "a click on the backdrop leaves Font over it");

        #[cfg(not(target_arch = "wasm32"))]
        {
            h.state_mut().browse_builtin("picture");
            h.frames(3);
            let browser = egui::LayerId::new(egui::Order::Foreground, egui::Id::new("builtin_file_browser"));
            assert_eq!(from(&h, backdrop(0)), [backdrop(0), dialog, backdrop(1), browser], "the browser is over Font, with its own backdrop");
        }
    }

    /// Go To opened again by its shortcut (also from the ribbon or the status bar) after a click
    /// on the page gave the page the keyboard: its page box takes the keyboard as it opens.
    #[test]
    fn opening_go_to_again_gives_its_box_the_keyboard() {
        let mut h = harness();
        h.state_mut().run("ui.dialog", json!({"name": "goto"})).unwrap();
        h.frames(3);
        let p = on_the_page(&h);
        click(&mut h, p);
        assert!(h.state().canvas.focused, "a click on the page gives it the keyboard");
        h.state_mut().run("ui.dialog", json!({"name": "goto"})).unwrap();
        h.frames(3);
        type_letter(&mut h, Key::Num2, "2");
        assert!(matches!(&h.state().dialog, Some(Dialog::Find { goto_target, .. }) if goto_target == "2"), "the digit is in the box");
        assert_eq!(body(&h), TEXT, "and not in the document");
    }

    /// The input's boundary, on event lists: after an event that may open a window everything
    /// waits for the next frame (what waited comes first next time); plain typing on the page, a
    /// held key's repeat, a modifier key, a key coming up but V, typed text and the wheel don't
    /// stop it, a key with Ctrl, a function key, V coming up, a paste, a button's release, a touch
    /// and a screen reader's action do; while the modal windows aren't drawn yet, everything waits.
    #[test]
    fn input_after_an_event_that_may_open_a_window_waits_for_the_next_frame() {
        let key = |key, pressed, modifiers| Event::Key { key, physical_key: None, pressed, repeat: false, modifiers };
        let at = egui::pos2(10.0, 10.0);
        let button = |pressed| Event::PointerButton { pos: at, button: egui::PointerButton::Primary, pressed, modifiers: Modifiers::NONE };
        let names = |events: &[Event]| events.iter().map(|e| format!("{e:?}")).collect::<Vec<_>>();
        let pace = |frame: &mut Vec<Event>, held: &mut Vec<Event>, typing: bool| super::pace_input(frame, 0, held, true, typing);
        let ctrl_d = || key(Key::D, true, Modifiers::CTRL | Modifiers::COMMAND);
        // A shortcut ends the frame's input: the letter, the click and the rest wait, then come first.
        let mut held = Vec::new();
        let mut frame = vec![Event::PointerMoved(at), ctrl_d(), button(true), Event::Text("x".into()), button(false)];
        assert!(pace(&mut frame, &mut held, true));
        assert_eq!(names(&frame), names(&[Event::PointerMoved(at), ctrl_d()]));
        assert_eq!(names(&held), names(&[button(true), Event::Text("x".into()), button(false)]));
        let mut frame = vec![Event::PointerMoved(at)];
        assert!(pace(&mut frame, &mut held, true), "the button's release, a click, ends that frame's input in turn");
        assert_eq!(names(&frame), names(&[button(true), Event::Text("x".into()), button(false)]));
        let mut frame = Vec::new();
        assert!(!pace(&mut frame, &mut held, true));
        assert_eq!(names(&frame), names(&[Event::PointerMoved(at)]));
        // Plain typing on the page, a repeat and a modifier key go on; elsewhere a letter may open a window.
        let typed = || key(Key::A, true, Modifiers::SHIFT);
        let repeat = || Event::Key { key: Key::D, physical_key: None, pressed: true, repeat: true, modifiers: Modifiers::CTRL };
        let shift = || key(Key::ShiftLeft, true, Modifiers::SHIFT);
        let wheel = || Event::MouseWheel {
            unit: egui::MouseWheelUnit::Point,
            delta: egui::vec2(0.0, -3.0),
            phase: egui::TouchPhase::Move,
            modifiers: Modifiers::NONE,
        };
        let touch = |phase| Event::Touch { device_id: egui::TouchDeviceId(0), id: egui::TouchId(0), phase, pos: at, force: None };
        for (mut frame, typing) in [
            (vec![typed(), typed(), button(true)], true),
            (vec![repeat(), button(true)], true),
            (vec![shift(), button(true)], true),
            (vec![key(Key::D, false, Modifiers::CTRL), button(true)], true),
            (vec![Event::Text("x".into()), wheel(), Event::PointerGone, button(true)], false),
            (vec![touch(egui::TouchPhase::Start), touch(egui::TouchPhase::Move), button(true)], true),
        ] {
            let all = names(&frame);
            assert!(!pace(&mut frame, &mut held, typing), "{all:?}");
            assert_eq!(names(&frame), all);
        }
        // A letter with the keyboard off the page, a function key, Alt with a letter, V coming up
        // (Mod+Alt+V's, also after Mod came up), a paste (Mod+Alt+V arrives as one), a click, a
        // touch ending, a screen reader's action.
        let action = Event::AccessKitActionRequest(egui::accesskit::ActionRequest {
            action: egui::accesskit::Action::Click,
            target_tree: egui::accesskit::TreeId::ROOT,
            target_node: egui::accesskit::NodeId(1),
            data: None,
        });
        for (opener, typing) in [
            (typed(), false),
            (key(Key::F12, true, Modifiers::NONE), true),
            (key(Key::A, true, Modifiers::ALT), true),
            (key(Key::V, false, Modifiers::NONE), true),
            (Event::Paste("x".into()), true),
            (button(false), true),
            (touch(egui::TouchPhase::End), true),
            (touch(egui::TouchPhase::Cancel), true),
            (action, true),
        ] {
            let mut frame = vec![opener.clone(), button(true)];
            assert!(pace(&mut frame, &mut held, typing), "{opener:?} ends the frame's input");
            assert_eq!(names(&frame), names(&[opener]));
            held.clear();
        }
        // A key's text comes with it (a KeyTip's letter takes both): the frame ends after the text.
        let mut frame = vec![key(Key::B, true, Modifiers::NONE), Event::Text("b".into()), button(true)];
        assert!(pace(&mut frame, &mut held, false));
        assert_eq!(names(&frame), names(&[key(Key::B, true, Modifiers::NONE), Event::Text("b".into())]));
        assert_eq!(names(&held), names(&[button(true)]));
        held.clear();
        // Not drawn yet: everything waits.
        let mut frame = vec![key(Key::A, true, Modifiers::NONE), Event::PointerMoved(at)];
        assert!(super::pace_input(&mut frame, 0, &mut held, false, true));
        assert!(frame.is_empty());
        assert_eq!(held.len(), 2);
        held.clear();
        // A frame of logic alone kept the input it was given (the first `earlier` events): what
        // waited comes after that, before the new input.
        let mut held = vec![button(true)];
        let mut frame = vec![key(Key::A, false, Modifiers::NONE), Event::PointerMoved(at)];
        assert!(!super::pace_input(&mut frame, 1, &mut held, true, true));
        assert_eq!(names(&frame), names(&[key(Key::A, false, Modifiers::NONE), button(true), Event::PointerMoved(at)]));
        // A fast mouse's and a touchpad's many moves wait merged, as the desktop sends them, one
        // kind after another: the last position of a run, raw motion and the wheel added up.
        let mut frame = vec![ctrl_d()];
        for _ in 0..super::MAX_HELD {
            frame.extend([Event::PointerMoved(at), Event::MouseMoved(egui::vec2(1.0, 0.0)), wheel()]);
        }
        frame.push(button(true));
        assert!(pace(&mut frame, &mut held, true));
        assert_eq!(names(&frame), names(&[ctrl_d()]));
        let turned = Event::MouseWheel {
            unit: egui::MouseWheelUnit::Point,
            delta: egui::vec2(0.0, -3.0 * super::MAX_HELD as f32),
            phase: egui::TouchPhase::Move,
            modifiers: Modifiers::NONE,
        };
        assert_eq!(names(&held), names(&[Event::PointerMoved(at), Event::MouseMoved(egui::vec2(super::MAX_HELD as f32, 0.0)), turned, button(true)]));
        // Past the most input that may wait, merged, the newest presses go first and every release
        // stays (no button stays down for the app): none of it goes on early.
        let mut held = Vec::new();
        let mut frame = vec![ctrl_d()];
        let pairs = super::MAX_HELD / 2 + 10;
        for _ in 0..pairs {
            frame.extend([button(true), button(false)]);
        }
        assert!(pace(&mut frame, &mut held, true));
        assert_eq!(names(&frame), names(&[ctrl_d()]));
        assert_eq!(held.len(), super::MAX_HELD);
        assert_eq!(held.iter().filter(|e| matches!(e, Event::PointerButton { pressed: false, .. })).count(), pairs, "every release waits");
        assert!(matches!(held.last(), Some(Event::PointerButton { pressed: false, .. })), "the newest presses are dropped");
        // Not drawn yet, the same limit.
        let mut frame: Vec<Event> = std::iter::repeat_with(|| button(true)).take(super::MAX_HELD + 5).collect();
        assert!(super::pace_input(&mut frame, 0, &mut held, false, true));
        assert!(frame.is_empty());
        assert_eq!(held.len(), super::MAX_HELD);
    }

    /// The window sends a held key's presses as first ones: they are marked as repeats, so a held
    /// shortcut's repeats don't each end a frame's input and pile up; a key coming up, or the
    /// window losing focus, starts again.
    #[test]
    fn a_held_keys_presses_are_repeats() {
        let press = |key| Event::Key { key, physical_key: None, pressed: true, repeat: false, modifiers: Modifiers::CTRL | Modifiers::SHIFT };
        let up = |key| Event::Key { key, physical_key: None, pressed: false, repeat: false, modifiers: Modifiers::NONE };
        let repeats = |events: &[Event]| events.iter().map(|e| matches!(e, Event::Key { repeat: true, .. })).collect::<Vec<_>>();
        let mut down = Vec::new();
        let mut frame = vec![press(Key::ArrowRight), press(Key::ArrowRight)];
        super::mark_repeats(&mut frame, &mut down);
        assert_eq!(repeats(&frame), [false, true]);
        let mut frame = vec![press(Key::ArrowRight), up(Key::ArrowRight), press(Key::ArrowRight)];
        super::mark_repeats(&mut frame, &mut down);
        assert_eq!(repeats(&frame), [true, false, false], "held from the last frame, then pressed again");
        let mut frame = vec![Event::WindowFocused(false), press(Key::ArrowRight)];
        super::mark_repeats(&mut frame, &mut down);
        assert_eq!(repeats(&frame), [false, false], "the window lost the keyboard meanwhile");
        // The repeats of a held Ctrl+Shift+Right go on in one frame.
        let mut frame = vec![press(Key::ArrowRight), press(Key::ArrowRight), press(Key::ArrowRight)];
        super::mark_repeats(&mut frame, &mut down);
        let mut held = Vec::new();
        assert!(!super::pace_input(&mut frame, 0, &mut held, true, true));
        assert_eq!(frame.len(), 3);
    }

    /// A press that ends behind a modal dialog: the pointer leaves for a frame, and the frame's
    /// pointer input (the release) waits, before what waited already.
    #[test]
    fn a_press_ends_with_the_pointer_leaving_for_a_frame() {
        let at = egui::pos2(10.0, 10.0);
        let release = Event::PointerButton { pos: at, button: egui::PointerButton::Primary, pressed: false, modifiers: Modifiers::NONE };
        let names = |events: &[Event]| events.iter().map(|e| format!("{e:?}")).collect::<Vec<_>>();
        let mut frame = vec![Event::Text("x".into()), Event::PointerMoved(at), release.clone()];
        let mut held = vec![Event::PointerGone];
        super::end_press(&mut frame, &mut held);
        assert_eq!(names(&frame), names(&[Event::Text("x".into()), Event::PointerGone]));
        assert_eq!(names(&held), names(&[Event::PointerMoved(at), release, Event::PointerGone]));
    }

    /// A button held down on the page or on the ribbon as a modal dialog opens (its shortcut
    /// pressed with the button down) clicks nothing when it comes up over the dialog: no menu
    /// behind it, no Bold. Without the dialog the same press and release do their part.
    #[test]
    fn a_button_held_as_a_modal_dialog_opens_clicks_nothing_behind_it() {
        use egui_kittest::kittest::Queryable;
        let mut h = harness();
        // The frames here are a quarter of a second each: without this, the press would be too
        // long for a click by the time it comes up, dialog or not.
        h.ctx.options_mut(|o| o.input_options.max_click_duration = 60.0);
        let depth = h.state().session.undo_depth();
        reset_behind(&mut h, depth);
        let bold = h.query_all_by_label("Bold").map(|n| n.rect()).min_by(|a, b| a.min.y.total_cmp(&b.min.y)).expect("Bold on the ribbon");
        let page = on_the_page(&h);
        for (what, at, button) in
            [("a right-click on the page", page, egui::PointerButton::Secondary), ("a click on Bold", bold.center(), egui::PointerButton::Primary)]
        {
            let e = |pressed| Event::PointerButton { pos: at, button, pressed, modifiers: Modifiers::NONE };
            let before = behind(&h);
            one_frame(&mut h, [Event::PointerMoved(at), e(true)]);
            one_frame(&mut h, [e(false)]);
            assert_ne!(behind(&h), before, "{what}: without a dialog it does something");
            reset_behind(&mut h, depth);
            let before = behind(&h);
            one_frame(&mut h, [Event::PointerMoved(at), e(true)]);
            one_frame(&mut h, key_events(Key::D, mod_key()));
            assert!(matches!(h.state().dialog, Some(Dialog::Font { .. })), "{what}: Mod+D opens Font");
            one_frame(&mut h, [e(false)]);
            assert_eq!(behind(&h), before, "{what}: the release behind Font clicks nothing");
            reset_behind(&mut h, depth);
        }
    }

    /// The window behind a modal dialog takes no key, also where it reads keys itself: Escape in
    /// the dialog closes the dialog alone, leaving Draw Table on and the File page open.
    #[test]
    fn keys_dont_reach_the_window_behind_a_modal_dialog() {
        let mut h = harness();
        h.state_mut().run("table.draw", json!({"on": true})).unwrap();
        h.state_mut().run("ui.dialog", json!({"name": "paragraph"})).unwrap();
        h.frames(3);
        press(&mut h, Key::Escape, Modifiers::NONE);
        assert!(h.state().dialog.is_none(), "Escape cancels Paragraph");
        assert!(h.state().canvas.table_tool.is_some(), "Draw Table stays on");

        h.state_mut().run("table.draw", json!({"on": false})).unwrap();
        h.state_mut().ui.backstage = true;
        h.frames(2);
        h.state_mut().run("ui.dialog", json!({"name": "font"})).unwrap();
        h.frames(3);
        press(&mut h, Key::Escape, Modifiers::NONE);
        assert!(h.state().dialog.is_none(), "Escape cancels Font");
        assert!(h.state().ui.backstage, "the File page stays open");
    }

    /// The panels the page shows over itself (here the comment card when only the selected comment
    /// is shown) are under the backdrop, on `Order::Middle`. The layers
    /// behind the dialog are `behind` it, and none is once it has closed.
    #[test]
    fn the_backdrop_stays_over_what_the_page_shows_over_itself() {
        let mut h = harness();
        h.state_mut().run("select.text", json!({"text": "kota"})).unwrap();
        h.state_mut().run("review.newComment", json!({"text": "Note"})).unwrap();
        h.state_mut().run("view.commentsPane", json!({"value": false})).unwrap();
        h.state_mut().run("review.trackingOptions", json!({"commentDisplay": "selected"})).unwrap();
        h.frames(4);
        // On `Order::Middle`: a card over everything (`Foreground`) would be another layer.
        let card = egui::LayerId::new(egui::Order::Middle, egui::Id::new("floating_comment"));
        assert!(h.ctx.memory(|m| m.areas().is_visible(&card)), "the comment card is shown over the page");
        h.state_mut().run("ui.dialog", json!({"name": "font"})).unwrap();
        h.frames(3);
        let backdrop = egui::LayerId::new(egui::Order::Foreground, egui::Id::new(("wordcraft.modalBackdrop", 0usize)));
        let dialog = egui::LayerId::new(egui::Order::Foreground, egui::Id::new(("dialog", "Font")));
        let order: Vec<egui::LayerId> = h.ctx.memory(|m| m.layer_ids().filter(|l| m.areas().is_visible(l)).collect());
        let at = |layer: egui::LayerId| order.iter().position(|l| *l == layer).unwrap_or_else(|| panic!("{layer:?} is shown: {order:?}"));
        assert!(at(card) < at(backdrop) && at(dialog) == at(backdrop) + 1, "the card is under the backdrop: {order:?}");
        assert!(super::behind(&h.ctx, card) && super::behind(&h.ctx, egui::LayerId::background()), "the card and the page are behind Font");
        assert!(!super::behind(&h.ctx, dialog), "Font isn't");
        press(&mut h, Key::Escape, Modifiers::NONE);
        assert!(h.state().dialog.is_none(), "Escape cancels");
        assert!(!super::behind(&h.ctx, egui::LayerId::background()), "nothing is behind a closed dialog");
    }

    /// A screen reader's click names its widget, so no backdrop stops it: behind a modal dialog it
    /// clicks nothing (Bold on the ribbon doesn't apply), and the dialog's own widgets still take
    /// theirs. Without the dialog the same click applies Bold.
    #[test]
    fn a_screen_readers_click_doesnt_reach_the_window_behind_a_modal_dialog() {
        use egui_kittest::kittest::Queryable;
        let mut h = harness();
        let depth = h.state().session.undo_depth();
        reset_behind(&mut h, depth);
        let click_bold = |h: &mut Harness<'static, WordApp>| {
            {
                let bold = h.query_all_by_label("Bold").min_by(|a, b| a.rect().min.y.total_cmp(&b.rect().min.y)).expect("Bold on the ribbon");
                bold.click_accesskit();
            }
            h.frames(2);
        };
        click_bold(&mut h);
        assert_eq!(h.state().session.undo_depth(), depth + 1, "without a dialog the click applies Bold");
        reset_behind(&mut h, depth);
        h.state_mut().run("ui.dialog", json!({"name": "font"})).unwrap();
        h.frames(3);
        click_bold(&mut h);
        assert!(matches!(h.state().dialog, Some(Dialog::Font { .. })), "Font stays open");
        assert_eq!(h.state().session.undo_depth(), depth, "Bold behind Font takes no click");
        let dialog = h.ctx.memory(|m| m.area_rect(egui::Id::new(("dialog", "Font")))).expect("Font is shown");
        {
            let cancel = h.query_all_by_label("Cancel").find(|n| dialog.contains(n.rect().center())).expect("Font's Cancel");
            cancel.click_accesskit();
        }
        h.frames(2);
        assert!(h.state().dialog.is_none(), "the dialog's own button takes the screen reader's click");
    }

    /// A file dropped on the window while a modal dialog is open is refused, as a disabled system
    /// window refuses it; without the dialog it comes through.
    #[test]
    fn a_file_dropped_behind_a_modal_dialog_is_refused() {
        /// A file as the window hands it over, never read here.
        #[derive(Debug)]
        struct Dropped;
        impl egui::DroppedFile for Dropped {
            fn path(&self) -> &std::path::Path {
                std::path::Path::new("picture.png")
            }

            fn bytes(&self) -> Result<Vec<u8>, String> {
                Err("not read in this test".into())
            }
        }
        let mut h = harness();
        let dropped = || egui::RawInput {
            dropped_files: vec![std::sync::Arc::new(Dropped) as egui::DroppedFileHandle],
            hovered_files: vec![egui::HoveredFile { path: Some("picture.png".into()), ..Default::default() }],
            ..Default::default()
        };
        let mut raw = dropped();
        h.state_mut().raw_input_hook(&mut raw);
        assert_eq!((raw.dropped_files.len(), raw.hovered_files.len()), (1, 1), "without a dialog the file comes through");
        h.state_mut().run("ui.dialog", json!({"name": "font"})).unwrap();
        let mut raw = dropped();
        h.state_mut().raw_input_hook(&mut raw);
        assert!(raw.dropped_files.is_empty() && raw.hovered_files.is_empty(), "Font open: the file is refused");
    }

    /// Closing the window (its close button, Alt+F4, Mod+Q) waits for a modal dialog to be
    /// answered: the dialog stays, and no Save Changes prompt takes its place.
    #[test]
    fn the_window_doesnt_close_behind_a_modal_dialog() {
        let mut h = harness();
        assert!(h.state().session.dirty, "the typed text is an unsaved change");
        h.state_mut().run("ui.dialog", json!({"name": "font"})).unwrap();
        h.frames(3);
        assert!(!h.state_mut().close_requested(), "the window stays");
        assert!(matches!(h.state().dialog, Some(Dialog::Font { .. })), "Font stays open, no prompt in its place");
        press(&mut h, Key::Escape, Modifiers::NONE);
        assert!(!h.state_mut().close_requested(), "unsaved changes: the window asks first");
        assert!(matches!(h.state().dialog, Some(Dialog::SaveChanges { .. })), "once Font is gone, closing asks to save");
    }

    /// An Alt tap behind the Colors dialog (open alone, from the ribbon) arms no KeyTips on the
    /// ribbon behind it; without the dialog the same tap does.
    #[cfg(not(target_os = "macos"))]
    #[test]
    fn an_alt_tap_behind_the_colors_dialog_arms_no_keytips() {
        let mut h = harness();
        let tap = |h: &mut Harness<'static, WordApp>| {
            h.event(Event::ModifiersChanged(Modifiers::ALT));
            h.event(Event::ModifiersChanged(Modifiers::NONE));
            h.frames(2);
        };
        tap(&mut h);
        assert_ne!(h.state().ui.keytips, crate::keytips::Phase::Off, "without a dialog the tap arms KeyTips");
        press(&mut h, Key::Escape, Modifiers::NONE);
        assert_eq!(h.state().ui.keytips, crate::keytips::Phase::Off);
        h.state_mut().colors = Some(crate::colors::ColorsDialog::new(None, crate::colors::Target::cmd("format.fontColor")));
        h.frames(3);
        tap(&mut h);
        assert_eq!(h.state().ui.keytips, crate::keytips::Phase::Off, "Colors open: no KeyTips behind it");
        // KeyTips showing as Colors opens (an agent opens it) end, as menus do.
        h.state_mut().colors = None;
        h.frames(3);
        tap(&mut h);
        assert_ne!(h.state().ui.keytips, crate::keytips::Phase::Off, "KeyTips show");
        h.state_mut().colors = Some(crate::colors::ColorsDialog::new(None, crate::colors::Target::cmd("format.fontColor")));
        h.frames(3);
        assert_eq!(h.state().ui.keytips, crate::keytips::Phase::Off, "Colors opening ends them");
    }

    /// Tab in SmartArt's Text Pane is read before anything is drawn (`WordApp::logic`): with a
    /// modal dialog opened over the pane (by an agent) it doesn't demote the item the pane had,
    /// when it comes in the dialog's first frame nor later.
    #[test]
    fn tab_in_the_smartart_text_pane_doesnt_reach_it_behind_a_modal_dialog() {
        use egui_kittest::kittest::Queryable;
        let mut h = harness();
        h.state_mut().run("insert.smartArt", json!({"layout": "hierarchy"})).unwrap();
        h.state_mut().run("smartArt.textPane", json!({"value": true})).unwrap();
        h.frames(3);
        let levels = |h: &Harness<'static, WordApp>| {
            wordcraft_engine::cmd::smart_art::selected_smart_art(&h.state().session).map(|g| g.items.iter().map(|i| i.level).collect::<Vec<u8>>())
        };
        {
            let row =
                h.query_all(egui_kittest::kittest::By::new().role(egui::accesskit::Role::TextInput).value("Build")).next().expect("the Build row");
            row.focus();
        }
        h.frames(2);
        assert!(h.state().smart_art_row_focused, "the pane's row has the keyboard");
        let start = levels(&h);
        h.state_mut().run("ui.dialog", json!({"name": "paragraph"})).unwrap();
        one_frame(&mut h, key_events(Key::Tab, Modifiers::NONE));
        assert!(matches!(h.state().dialog, Some(Dialog::Paragraph { .. })), "Paragraph stays open");
        assert_eq!(levels(&h), start, "Tab doesn't demote the item behind Paragraph");
        // Later too, with the row's hold on the keyboard as `logic` reads it (the dialog took the
        // focus as it opened): the Tab is taken away before the pane looks.
        assert!(h.state().smart_art_item.is_some(), "the pane has an item");
        h.state_mut().smart_art_row_focused = true;
        one_frame(&mut h, key_events(Key::Tab, Modifiers::NONE));
        assert!(matches!(h.state().dialog, Some(Dialog::Paragraph { .. })), "Paragraph stays open");
        assert_eq!(levels(&h), start, "Tab doesn't demote the item behind Paragraph");
    }

    /// A window an agent opens over a dialog (here the Colors dialog over Paragraph) keeps the
    /// click that comes in its frame from the dialog under it: the click waits until the Colors
    /// dialog and its backdrop are drawn, and lands outside it (which closes it, as a click
    /// outside an `egui::Modal` does), not on Paragraph's Cancel. Without the Colors dialog the
    /// same click cancels Paragraph.
    #[test]
    fn a_window_opened_over_a_dialog_takes_the_click_from_it() {
        use egui_kittest::kittest::Queryable;
        let mut h = harness();
        let cancel_at = |h: &mut Harness<'static, WordApp>| {
            h.state_mut().run("ui.dialog", json!({"name": "paragraph"})).unwrap();
            h.frames(3);
            let dialog = h.ctx.memory(|m| m.area_rect(egui::Id::new(("dialog", "Paragraph")))).expect("Paragraph is shown");
            h.query_all_by_label("Cancel").map(|n| n.rect().center()).find(|c| dialog.contains(*c)).expect("Paragraph's Cancel")
        };
        let click = |at: egui::Pos2| {
            let e = |pressed| Event::PointerButton { pos: at, button: egui::PointerButton::Primary, pressed, modifiers: Modifiers::NONE };
            [Event::PointerMoved(at), e(true), e(false)]
        };
        let at = cancel_at(&mut h);
        one_frame(&mut h, click(at));
        assert!(h.state().dialog.is_none(), "without the Colors dialog the click cancels Paragraph");
        let at = cancel_at(&mut h);
        h.state_mut().colors = Some(crate::colors::ColorsDialog::new(None, crate::colors::Target::cmd("format.fontColor")));
        one_frame(&mut h, click(at));
        h.frames(2);
        assert!(matches!(h.state().dialog, Some(Dialog::Paragraph { .. })), "Paragraph under it isn't cancelled");
    }

    /// A file browser opened from a modal dialog and cancelled leaves the dialog the top window in
    /// the frame it closes in (Tab and focus stay within the dialog).
    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn a_cancelled_file_browser_leaves_the_dialog_the_top_window() {
        use egui_kittest::kittest::Queryable;
        let mut h = harness();
        h.state_mut().run("ui.dialog", json!({"name": "paragraph"})).unwrap();
        h.frames(3);
        h.state_mut().browse_builtin("picture");
        h.frames(3);
        let browser = egui::LayerId::new(egui::Order::Foreground, egui::Id::new("builtin_file_browser"));
        assert_eq!(h.ctx.memory(|m| m.top_modal_layer()), Some(browser), "the browser is the top window");
        {
            let area = h.ctx.memory(|m| m.area_rect(browser.id)).expect("the browser is shown");
            // Paragraph's own Cancel may lie under the browser too: the browser's is drawn last.
            let cancel = h.query_all_by_label("Cancel").rfind(|n| area.contains(n.rect().center())).expect("the browser's Cancel");
            cancel.click_accesskit();
        }
        h.frame();
        assert!(!h.state().file_dialog_open(), "Cancel closes the browser");
        let paragraph = egui::LayerId::new(egui::Order::Foreground, egui::Id::new(("dialog", "Paragraph")));
        assert_eq!(h.ctx.memory(|m| m.top_modal_layer()), Some(paragraph), "Paragraph is the top window at once");
    }

    /// The first modal dialog of a session shows its backdrop for the first time, and egui measures
    /// a new layer first, so it takes no click in the next frame: that frame's click waits a frame
    /// more instead of reaching the page under it.
    #[test]
    fn the_first_backdrop_of_a_session_takes_the_next_click() {
        let mut h = harness();
        let p = on_the_page(&h);
        h.event(Event::PointerMoved(p));
        h.frames(2);
        let start = format!("{:?}", h.state().session.sel);
        h.input_mut().events.extend(key_events(Key::D, mod_key()));
        h.frame();
        assert!(matches!(h.state().dialog, Some(Dialog::Font { .. })), "Mod+D opens Font");
        let e = |pressed| Event::PointerButton { pos: p, button: egui::PointerButton::Primary, pressed, modifiers: Modifiers::NONE };
        h.input_mut().events.extend([e(true), e(false)]);
        h.frame();
        assert_eq!(format!("{:?}", h.state().session.sel), start, "the click in the next frame doesn't reach the page");
        h.frames(3);
        assert_eq!(format!("{:?}", h.state().session.sel), start, "nor once it comes through");
    }

    /// A dialog closed by its close box gives the page the keyboard at once: a letter in the very
    /// next frame is typed into the document.
    #[test]
    fn a_letter_right_after_the_close_box_reaches_the_document() {
        let mut h = harness();
        press(&mut h, Key::D, mod_key());
        assert!(matches!(h.state().dialog, Some(Dialog::Font { .. })), "Mod+D opens Font");
        let hit = egui::Id::new(("dialog", "Font")).with("close_box").with("hit");
        let at = h.ctx.read_response(hit).expect("Font's close box").rect.center();
        h.event(Event::PointerMoved(at));
        h.frames(2);
        let button = |pressed| Event::PointerButton { pos: at, button: egui::PointerButton::Primary, pressed, modifiers: Modifiers::NONE };
        h.input_mut().events.extend([button(true), button(false)]);
        h.frame();
        assert!(h.state().dialog.is_none(), "the close box closes Font");
        h.input_mut().events.extend(letter(Key::Y, "y"));
        h.frame();
        h.frames(2);
        assert_eq!(body(&h), format!("{TEXT}y"), "the letter right after it reaches the document");
    }

    /// Find and Replace takes the keyboard as it opens: Enter in its box finds the next match.
    /// A click on the page gives the page the keyboard with Find still open; then Enter starts a
    /// paragraph rather than finding the next match, and Escape leaves the dialog open.
    #[test]
    fn a_modeless_dialog_leaves_enter_and_escape_to_the_page() {
        let mut h = harness();
        h.state_mut().run("ui.dialog", json!({"name": "find"})).unwrap();
        h.frames(2);
        assert!(!h.state().canvas.focused, "Find takes the keyboard");
        h.event(Event::Text("kota".into()));
        h.frames(2);
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
        h.frames(2);
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
        h.frames(3);
        type_letter(&mut h, Key::Y, "y");
        assert_eq!(body(&h), format!("{TEXT}y"));
    }

    /// The Colors dialog is modal too: a letter doesn't reach the document behind it.
    #[test]
    fn the_colors_dialog_takes_every_key() {
        let mut h = harness();
        h.state_mut().colors = Some(crate::colors::ColorsDialog::new(None, crate::colors::Target::cmd("format.fontColor")));
        h.frames(2);
        type_letter(&mut h, Key::X, "x");
        assert_eq!(body(&h), TEXT);
        h.state_mut().colors = None;
        h.frames(3);
        type_letter(&mut h, Key::Y, "y");
        assert_eq!(body(&h), format!("{TEXT}y"), "the page has the keyboard back");
    }

    /// Input from outside the window runs after the frame's own (`WordApp::outside_input`): with
    /// an agent's Paragraph opening in the frame of a letter and Enter on the page, they came first
    /// and are the page's (typed, a new paragraph), and Enter isn't the new dialog's OK; a letter
    /// in the next frames goes to the dialog, not the document.
    #[test]
    fn input_in_the_frame_an_agent_opens_a_dialog_in_is_the_pages() {
        let mut h = harness();
        let (tx, rx) = std::sync::mpsc::channel();
        h.state_mut().control_rx = Some(rx);
        let (req, _reply) = crate::control::ControlRequest::new("engine.execute", json!({"command": "ui.dialog", "params": {"name": "paragraph"}}));
        tx.send(req).unwrap();
        h.input_mut().events.extend(letter(Key::X, "x").into_iter().chain(key_events(Key::Enter, Modifiers::NONE)));
        h.frame();
        assert!(matches!(h.state().dialog, Some(Dialog::Paragraph { .. })), "the agent's Paragraph opens");
        assert_eq!(body(&h), format!("{TEXT}x\n"), "the frame's letter and Enter came first: the page's");
        h.settle();
        h.frames(2);
        assert!(matches!(h.state().dialog, Some(Dialog::Paragraph { .. })), "Enter wasn't Paragraph's OK");
        h.input_mut().events.extend(letter(Key::Y, "y"));
        h.frame();
        h.settle();
        assert_eq!(body(&h), format!("{TEXT}x\n"), "a letter after it opened doesn't reach the document");
    }

    /// A modal window opened from outside the window takes the keyboard from the page at once
    /// (`WordApp::from_outside`), and the page gets it back as the window closes, also from
    /// outside: a letter typed then reaches the document.
    #[test]
    fn the_page_gets_the_keyboard_back_after_a_window_opened_from_outside() {
        let mut h = harness();
        let (tx, rx) = std::sync::mpsc::channel();
        h.state_mut().control_rx = Some(rx);
        let (req, _reply) = crate::control::ControlRequest::new("engine.execute", json!({"command": "ui.dialog", "params": {"name": "font"}}));
        tx.send(req).unwrap();
        h.frame();
        assert!(matches!(h.state().dialog, Some(Dialog::Font { .. })), "the agent's Font opens");
        assert!(!h.state().canvas.focused, "the page has given up the keyboard");
        h.frames(3);
        h.state_mut().dialog = None;
        h.frames(3);
        assert!(h.state().canvas.focused, "the page has the keyboard back");
        type_letter(&mut h, Key::Y, "y");
        assert_eq!(body(&h), format!("{TEXT}y"));
    }

    /// The built-in file browser with no dialog under it (File › Open where the system has no file
    /// dialog) and Zotero's alert are modal windows: a click on the page and a letter don't reach
    /// the document behind them, also when they come in the frame after the window opens.
    #[test]
    fn the_file_browser_and_zoteros_alert_are_modal() {
        let mut h = harness();
        let p = on_the_page(&h);
        let clicked = |at: egui::Pos2| {
            let e = |pressed| Event::PointerButton { pos: at, button: egui::PointerButton::Primary, pressed, modifiers: Modifiers::NONE };
            [Event::PointerMoved(at), e(true), e(false)]
        };
        h.state_mut().run("select.text", json!({"text": "Ala"})).unwrap();
        h.frames(2);
        let sel = h.state().session.sel.clone();
        #[cfg(not(target_arch = "wasm32"))]
        {
            h.state_mut().browse_builtin("open");
            h.frame();
            assert!(super::open(h.state()), "the browser alone is a modal window");
            one_frame(&mut h, clicked(p).into_iter().chain(letter(Key::X, "x")));
            assert!(h.state().file_browser_open(), "the browser stays open");
            assert_eq!(h.state().session.sel, sel, "the click doesn't move the caret behind the browser");
            assert_eq!(body(&h), TEXT, "the letter doesn't reach the document behind the browser");
            h.state_mut().close_file_browser(None);
            h.frames(3);
        }
        let _answer = h.state_mut().zotero.ask("Update the bibliography?");
        h.frame();
        assert_eq!(super::top(h.state()), super::Top::ZoteroAlert, "Zotero's alert is a modal window");
        one_frame(&mut h, clicked(p).into_iter().chain(letter(Key::X, "x")));
        assert!(h.state().zotero.alert_open(), "the alert waits for its answer");
        assert_eq!(h.state().session.sel, sel, "the click doesn't move the caret behind the alert");
        assert_eq!(body(&h), TEXT, "the letter doesn't reach the document behind the alert");
    }

    /// The Read Aloud bar is the page's: behind a modal dialog a screen reader's click on it does
    /// nothing (the keys and actions come back only for the dialog), and without the dialog the
    /// same click moves to the next sentence.
    #[test]
    fn a_screen_readers_click_on_read_aloud_behind_a_modal_dialog_does_nothing() {
        use egui_kittest::kittest::Queryable;
        let mut h = harness();
        // No speech program: the reader moves only on the clicks.
        h.state_mut().session.read_aloud.backend = wordcraft_engine::speech::Backend::Hold;
        h.state_mut().run("document.setText", json!({"text": "One. Two. Three. Four."})).unwrap();
        h.state_mut().run("select.all", json!({})).unwrap();
        h.state_mut().run("select.collapse", json!({"end": false})).unwrap();
        h.state_mut().run("review.readAloud", json!({})).unwrap();
        // Paused, so only the clicks move it on.
        if h.state().session.read_aloud.state() == wordcraft_engine::speech::State::Playing {
            h.state_mut().run("readAloud.playPause", json!({})).unwrap();
        }
        h.frames(3);
        let next = |h: &mut Harness<'static, WordApp>| {
            h.query_by_label("Next sentence").expect("Read Aloud's Next sentence").click_accesskit();
            h.frames(2);
        };
        let index = |h: &Harness<'static, WordApp>| h.state().session.read_aloud.progress().0;
        let start = index(&h);
        next(&mut h);
        assert_eq!(index(&h), start + 1, "without a dialog the click moves to the next sentence");
        h.state_mut().run("ui.dialog", json!({"name": "font"})).unwrap();
        h.frames(3);
        next(&mut h);
        assert!(matches!(h.state().dialog, Some(Dialog::Font { .. })), "Font stays open");
        assert_eq!(index(&h), start + 1, "behind Font the click does nothing");
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
