//! Header & Footer tab: where the caret is in a header or footer, moving between sections'
//! headers, linking a section's header to the previous one, and the distances from the page edge.

use serde_json::{Value, json};
use wordcraft_doc::section::{HeaderSet, SectionProps};
use wordcraft_doc::{Document, PartKind, StoryRef};

use super::sel_result;
use crate::{CmdError, CmdResult, CommandSpec, Selection, Session, p};

pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new("hf.next", "Next Section", "Header & Footer › Navigation", |s, _| go(s, true)).pure(),
        CommandSpec::new("hf.previous", "Previous Section", "Header & Footer › Navigation", |s, _| go(s, false)).pure(),
        CommandSpec::new("hf.position", "Header/Footer Position", "Header & Footer › Position", position)
            .params(r#"{"header"?: pt (distance from the top of the page to the header), "footer"?: pt (from the bottom to the footer)} — each clamped to half the page height; returns both"#),
        CommandSpec::new("layout.linkToPrevious", "Link to Previous", "Header & Footer › Navigation", link_to_previous)
            .key(super::non_mac("Alt+Shift+R"))
            .params(r#"{"value"?: bool} (true: the header or footer the caret is in shows the previous section's; false: it gets its own copy; without a value it toggles), {"footer"?: bool} (outside a header or footer: the caret's section's footer instead of its header)"#),
    ]
}

/// Which section's header or footer the caret is in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HfAt {
    /// Index in `Document::sections()`.
    pub section: usize,
    pub header: bool,
}

/// Whether the caret is in a header or a footer (what brings up the Header & Footer tab).
pub fn in_header_footer(s: &Session) -> bool {
    matches!(s.sel.focus.story, StoryRef::Part(id) if matches!(s.doc.parts.get(&id).map(|p| &p.kind), Some(PartKind::Header | PartKind::Footer)))
}

fn holds(set: &HeaderSet, id: u32) -> bool {
    [set.default, set.first, set.even].contains(&Some(id))
}

/// The section and kind of the header or footer the caret is in. A part linked from several
/// sections is the one the page being edited shows; without a layout to ask, its first section.
pub fn hf_at(s: &Session) -> Option<HfAt> {
    if !in_header_footer(s) {
        return None;
    }
    let StoryRef::Part(id) = s.sel.focus.story else { return None };
    if let Some(page) = s.cached_layout().and_then(|l| l.pages.get(s.page_hint)) {
        if page.header_story == Some(id) {
            return Some(HfAt { section: page.section, header: true });
        }
        if page.footer_story == Some(id) {
            return Some(HfAt { section: page.section, header: false });
        }
    }
    s.doc.sections().iter().enumerate().find_map(|(section, (_, sp))| {
        if holds(&sp.headers, id) {
            Some(HfAt { section, header: true })
        } else if holds(&sp.footers, id) {
            Some(HfAt { section, header: false })
        } else {
            None
        }
    })
}

/// The last body block of the section whose header or footer the caret is in.
pub fn section_end(s: &Session) -> Option<usize> {
    let at = hf_at(s)?;
    s.doc.sections().get(at.section).map(|(end, _)| *end)
}

/// The section's properties as they apply to the header or footer the caret is in.
pub fn section_props(s: &Session) -> Option<SectionProps> {
    let at = hf_at(s)?;
    s.doc.sections().get(at.section).map(|(_, sp)| (*sp).clone())
}

/// Whether the header (or footer) the caret is in is linked to the previous section's.
pub fn linked_to_previous(s: &Session) -> bool {
    hf_at(s).is_some_and(|at| linked(s, at))
}

fn linked(s: &Session, at: HfAt) -> bool {
    s.doc.sections().get(at.section).is_some_and(|(_, sp)| (if at.header { sp.headers } else { sp.footers }).default.is_none())
}

/// What section `section` shows: its own header (or footer) parts, with those it leaves out taken
/// from the sections before it, like the layout does.
fn effective(doc: &Document, section: usize, header: bool) -> HeaderSet {
    let sections = doc.sections();
    let mut set = HeaderSet::default();
    for (_, sp) in sections.iter().take(section.saturating_add(1)).rev() {
        let o = if header { sp.headers } else { sp.footers };
        set.default = set.default.or(o.default);
        set.first = set.first.or(o.first);
        set.even = set.even.or(o.even);
    }
    let live = |id: Option<u32>| id.filter(|id| doc.parts.contains_key(id));
    HeaderSet { default: live(set.default), first: live(set.first), even: live(set.even) }
}

/// Puts the caret at the end of header or footer `id`, on the first page of section `section`.
fn enter(s: &mut Session, id: u32, section: usize) -> CmdResult {
    s.touch();
    // Laid out again, so which page shows this part (and so which section it is for) is current.
    if let Some(page) = s.layout().pages.iter().position(|pg| pg.section == section) {
        s.page_hint = page;
    }
    s.sel = Selection::caret(s.doc.end_of(StoryRef::Part(id)));
    Ok(json!({"story": id, "section": section}))
}

/// The header (or footer) of the next / previous section that differs from this one's.
fn go(s: &mut Session, forward: bool) -> CmdResult {
    let at = hf_at(s).ok_or_else(|| CmdError::Failed("the caret is not in a header or footer".into()))?;
    let StoryRef::Part(cur) = s.sel.focus.story else { return Err(CmdError::Failed("the caret is not in a header or footer".into())) };
    let n = s.doc.sections().len();
    let order: Vec<usize> = if forward { (at.section.saturating_add(1)..n).collect() } else { (0..at.section).rev().collect() };
    let found = order.into_iter().find_map(|j| effective(&s.doc, j, at.header).default.filter(|id| *id != cur).map(|id| (j, id)));
    match found {
        Some((j, id)) => enter(s, id, j),
        None => Err(CmdError::Failed(format!(
            "no {} section with its own {}",
            if forward { "next" } else { "previous" },
            if at.header { "header" } else { "footer" }
        ))),
    }
}

fn position(s: &mut Session, v: &Value) -> CmdResult {
    let end = section_end(s).unwrap_or_else(|| s.sel.focus.path.0.first().copied().unwrap_or(0) as usize);
    let sec = s.doc.section_mut(end);
    let max = (sec.page_h / 2.0).max(0.0);
    if let Some(x) = p::f32(v, "header") {
        sec.header = x.clamp(0.0, max);
    }
    if let Some(x) = p::f32(v, "footer") {
        sec.footer = x.clamp(0.0, max);
    }
    let out = json!({"header": sec.header, "footer": sec.footer});
    s.touch();
    Ok(out)
}

fn link_to_previous(s: &mut Session, v: &Value) -> CmdResult {
    // From the body it is the caret's section, header or (with `footer`) footer.
    let at = hf_at(s).unwrap_or_else(|| HfAt {
        section: s.doc.section_index_of(s.sel.focus.path.0.first().copied().unwrap_or(0) as usize),
        header: !p::bool(v, "footer").unwrap_or(false),
    });
    if at.section == 0 {
        return Err(CmdError::Failed("the first section has nothing to link to".into()));
    }
    let end = s.doc.sections().get(at.section).map(|(end, _)| *end).ok_or_else(|| CmdError::Failed("no such section".into()))?;
    let was = linked(s, at);
    let want = p::bool(v, "value").unwrap_or(!was);
    if want == was {
        return sel_result(s);
    }
    let kind = if at.header { PartKind::Header } else { PartKind::Footer };
    if want {
        // Its own parts go; the section shows the previous one's.
        let sec = s.doc.section_mut(end);
        let set = if at.header { &mut sec.headers } else { &mut sec.footers };
        let old = std::mem::take(set);
        for id in [old.default, old.first, old.even].into_iter().flatten() {
            if !s.doc.sections().iter().any(|(_, sp)| holds(&sp.headers, id) || holds(&sp.footers, id)) {
                s.doc.parts.remove(&id);
            }
        }
        let shown = effective(&s.doc, at.section, at.header).default;
        return match shown {
            Some(id) => enter(s, id, at.section),
            None => sel_result(s),
        };
    }
    // Its own copy of what it showed, so editing it leaves the previous section alone.
    let inherited = effective(&s.doc, at.section, at.header);
    let mut copies = HeaderSet::default();
    for (slot, from) in [(&mut copies.default, inherited.default), (&mut copies.first, inherited.first), (&mut copies.even, inherited.even)] {
        let Some(blocks) = from.and_then(|id| s.doc.parts.get(&id)).map(|part| part.blocks.clone()) else { continue };
        let id = s.doc.add_part(kind, blocks);
        if id == 0 {
            return Err(CmdError::Failed("no room for another header or footer".into()));
        }
        *slot = Some(id);
    }
    let sec = s.doc.section_mut(end);
    let set = if at.header { &mut sec.headers } else { &mut sec.footers };
    set.default = set.default.or(copies.default);
    set.first = set.first.or(copies.first);
    set.even = set.even.or(copies.even);
    match copies.default {
        Some(id) => enter(s, id, at.section),
        None => sel_result(s),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wordcraft_doc::Document;

    /// Two sections of one page each, a header on the first (the second links to it).
    fn two_sections() -> (Session, u32) {
        let mut s = Session::new(Document::new());
        s.run("text.insert", &json!({"text": "One"})).unwrap();
        s.run("layout.break", &json!({"kind": "nextPage"})).unwrap();
        s.run("text.insert", &json!({"text": "Two"})).unwrap();
        assert_eq!(s.layout().pages.len(), 2);
        let id = s.run("insert.editHeader", &json!({"page": 0})).unwrap()["story"].as_u64().unwrap() as u32;
        s.run("text.insert", &json!({"text": "Intro"})).unwrap();
        assert_eq!(s.doc.sections().len(), 2);
        (s, id)
    }

    fn header_text(s: &Session, id: u32) -> String {
        s.doc.plain_text(StoryRef::Part(id))
    }

    #[test]
    fn the_tab_belongs_to_headers_and_footers_only() {
        let (mut s, _) = two_sections();
        assert!(in_header_footer(&s));
        s.run("insert.closeHeader", &json!({})).unwrap();
        assert!(!in_header_footer(&s));
        assert!(hf_at(&s).is_none());
        assert!(s.run("hf.next", &json!({})).is_err(), "no header, no navigation");
    }

    /// Different First Page on a header applies to the section it belongs to, not the body block
    /// that happens to have the header paragraph's index.
    #[test]
    fn header_options_apply_to_the_headers_own_section() {
        let (mut s, _) = two_sections();
        s.run("insert.editHeader", &json!({"page": 1})).unwrap();
        assert_eq!(hf_at(&s), Some(HfAt { section: 1, header: true }));
        s.run("layout.differentFirstPage", &json!({"value": true})).unwrap();
        let secs = s.doc.sections();
        assert!(!secs[0].1.title_page);
        assert!(secs[1].1.title_page);
    }

    #[test]
    fn link_to_previous_unlinks_into_a_copy_and_links_back() {
        let (mut s, first) = two_sections();
        let parts = s.doc.parts.len();
        s.run("insert.editHeader", &json!({"page": 1})).unwrap();
        assert!(linked_to_previous(&s));
        s.run("layout.linkToPrevious", &json!({"value": false})).unwrap();
        assert!(!linked_to_previous(&s));
        let StoryRef::Part(copy) = s.sel.focus.story else { panic!("the caret moves into the copy") };
        assert_ne!(copy, first);
        assert_eq!(header_text(&s, copy), "Intro");
        s.run("text.insert", &json!({"text": " two"})).unwrap();
        assert_eq!(header_text(&s, first), "Intro", "the previous section's header is untouched");
        assert_eq!(s.layout().pages[1].header_story, Some(copy));
        // Back: the copy goes, the section shows the first header again.
        s.run("layout.linkToPrevious", &json!({})).unwrap();
        assert!(linked_to_previous(&s));
        assert_eq!(s.sel.focus.story, StoryRef::Part(first));
        assert_eq!(s.doc.parts.len(), parts);
        assert_eq!(s.layout().pages[1].header_story, Some(first));
        // One undo step each.
        s.run("edit.undo", &json!({})).unwrap();
        assert!(s.doc.parts.contains_key(&copy));
    }

    #[test]
    fn the_first_section_has_nothing_to_link_to() {
        let (mut s, _) = two_sections();
        s.run("insert.editHeader", &json!({"page": 0})).unwrap();
        assert!(s.run("layout.linkToPrevious", &json!({"value": true})).is_err());
    }

    #[test]
    fn next_and_previous_move_between_the_sections_own_headers() {
        let (mut s, first) = two_sections();
        s.run("insert.editHeader", &json!({"page": 1})).unwrap();
        s.run("layout.linkToPrevious", &json!({"value": false})).unwrap();
        let StoryRef::Part(copy) = s.sel.focus.story else { panic!("in the copy") };
        s.run("hf.previous", &json!({})).unwrap();
        assert_eq!(s.sel.focus.story, StoryRef::Part(first));
        assert_eq!(hf_at(&s).map(|a| a.section), Some(0));
        assert!(s.run("hf.previous", &json!({})).is_err(), "nothing before the first section");
        s.run("hf.next", &json!({})).unwrap();
        assert_eq!(s.sel.focus.story, StoryRef::Part(copy));
        assert_eq!(hf_at(&s).map(|a| a.section), Some(1));
        assert!(s.run("hf.next", &json!({})).is_err(), "nothing after the last section");
    }

    #[test]
    fn position_is_clamped_to_the_page() {
        let (mut s, _) = two_sections();
        let r = s.run("hf.position", &json!({"header": 36.0, "footer": 1.0e9})).unwrap();
        assert_eq!(r["header"].as_f64(), Some(36.0));
        assert!(r["footer"].as_f64().is_some_and(|f| f <= 396.0));
        let r = s.run("hf.position", &json!({"header": -5.0, "footer": "junk"})).unwrap();
        assert_eq!(r["header"].as_f64(), Some(0.0));
    }
}
