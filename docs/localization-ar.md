# Arabic interface

> **Last reviewed:** 2026-10-17 · **Last updated:** 2026-10-20 · **Change:** minor (review of #2677: reading order for stored, OS-facing and automation text; `fmt` values logical with isolates; assembled lines through `i18n::line`; 58 newer keys; previously minor: Arabic interface catalog, Arabic interface font fallback) · **Target:** WordCraft main (2026-10-20)

Choose **File › Options › Interface language › العربية**, or follow an Arabic system display
language (`ar`, `ar-SA`, `ar-EG`, `ar_SA.UTF-8` and the other regions). The catalog code is `ar`;
`ui.language` with `{"value":"ar"}` selects it explicitly, and `WORDCRAFT_LANGUAGE=ar-SA` does the
same for a headless screenshot or a launch while the preference is `auto`. Command ids, the text
already in a document, and paths a script or the user supplies do not change. Interface text that
reaches automation or files is localized but kept in reading order (see Word order): error
messages from commands, the control channel's `title`, and the default name of an untitled
document (`مستند1`).

`crates/ui-egui/src/i18n/ar.tsv` has 2,674 entries: the union of every key in every other catalog
(built-in style names, equation gallery labels, paper sizes, watermarks and citation styles
included). It is written from the meaning of the English labels in ordinary Arabic computing
vocabulary; no other product's Arabic interface, string table or terminology database was
consulted, and no other catalog was used as a source. Placeholders, `\n` breaks in ribbon labels
and dialog ellipses are kept (checked by `bundled_catalogs_are_well_formed`), and strings added
later fall back to English until they are translated.

The draft was made with Claude Code. **It still needs a native speaker to approve the terminology
and review the running app** before this line can say so.

## Interface font

The interface's Latin faces (Inter, Source Sans 3) have no Arabic letters, so without a fallback
every Arabic label, and «العربية» in the language list, drew as boxes. `theme::font_definitions_language`
now appends, after the CJK faces:

- the craft-fonts Arabic face (`wordcraft_fonts::ui_arabic_fonts`) when the build has one, else
- an installed Arabic font (`wordcraft_fonts::system_arabic_ui_font`): Segoe UI, Tahoma or Arial
  on Windows; SF Arabic, Geeza Pro or Arial on macOS; Noto Sans Arabic, Noto Naskh Arabic or
  DejaVu Sans elsewhere. The file is chosen once for the catalog's Arabic letters, like the system
  CJK fallback (#241, #486).

## Word order

egui 0.36 joins letters within a word but lays a line's words out left to right (no bidi), so
`الحفظ التلقائي` drew as `التلقائي الحفظ`. `i18n::bidi` (ported from PhotoCraft's `bidi_text`)
puts text into display order with the Unicode Bidirectional Algorithm, paragraph direction right
to left: words reversed, Latin names and numbers kept in order, punctuation and ellipses moved to
the left end, brackets mirrored, `\n` lines kept. Display order is only for egui to draw; it is
never stored, compared, typed over, handed to the operating system or reported to automation.

### Which variant to use

| Text | Use | Why |
|---|---|---|
| A label handed straight to egui (`ui.label`, buttons, menus, tooltips) | `tl!` / `t` / `tr` | display order, built once per catalog |
| Text stored or saved (file names, document text, form fields that are typed over or inserted into the document) | `t_logical` / `tr_logical` | `مستند1`, not `1مستند` |
| Text compared with what the user typed | `t_logical` | the user types in reading order |
| Window title and other OS-facing text (file dialog suggested names) | `t_logical` | the OS runs its own bidi |
| Control channel, CLI, MCP, `Err` returned to automation | `t_logical`, `fmt_logical` | scripts read reading order; the status bar gets `i18n::line` of the same text |
| A value passed to `fmt` | `t_logical`, user text as typed, numbers, or a `fmt_logical` result | `fmt` orders the whole line once |
| A line the code assembles (`format!`, `join`) from translated pieces | build from `t_logical`, wrap in `i18n::line` | two display-ordered pieces in logical sequence read wrong |
| A ribbon location | `location` to draw, `location_logical` inside a longer line | |

- `fmt`: values are logical. A right-to-left template (templates are kept as written) is filled
  and the whole line put in display order, each value wrapped in First Strong Isolate / Pop
  Directional Isolate (stripped afterwards) so it keeps its own direction: a path or number stays
  whole and left to right, a multi-word Arabic file or style name reads right to left with the
  rest. Left-to-right templates take the original one-pass path (no extra allocation).
- `title_stem()` (untitled: `t_logical("Document1")`) is reading order everywhere it goes: the
  suggested Save As / Export names, the default `.docx` name, the Rename dialog's field, the
  control channel's `title`, the native window title. The title bar egui draws composes it in
  reading order, shortens a long one in reading order (the name keeps its start, `…` at its
  end), and passes only the painted text through `i18n::line`; its accessibility label stays in
  reading order.
- Errors returned to automation are in reading order: download failure, `file.autosave`'s refusal
  (`AutoSaveBlock::reason_logical`; `reason` is its drawn form), the Save Changes prompt's failed
  or cancelled save (the prompt and status bar draw it), `ui.crop`.
- Assembled lines go through `i18n::line` (status, labels with a value after a colon, tab stop
  tooltips, the command search results). Pieces that feed them stay in reading order to the end:
  the property descriptions (`panes::describe`) are built with `t_logical`/`fmt_logical`; the
  tracked-formatting balloon and the Style Inspector's Direct formatting rows join them and draw
  each line through one `line`. Arrows, menu marks (`▾`, `◀`) and sample glyphs stay on
  their physical side, since the interface isn't mirrored.
- Command search matches the typed words against labels and locations in reading order
  (`t_logical`, `location_logical`).
- Tests: `arabic_is_drawn_in_display_order_but_kept_in_reading_order`,
  `arabic_untitled_name_is_saved_renamed_and_reported_in_reading_order` (suggested `.docx` and
  `.pdf` names, the Rename field, `file.autosave`'s error, the drawn title), and the `bidi` unit
  tests.

Trade-offs: the accessibility tree gets the display-order labels too, so a screen reader may read
the words of a multi-word Arabic label in reverse; DragValue suffixes (`نقطة`) stay after the number
as egui composes them. Fixing those needs the mirrored, bidi-aware layout below rather than
reordered strings.

## Conventions

- Modern Standard Arabic, neutral across regions. Commands are verbal nouns (`حفظ`, `إدراج جدول`,
  `تحديد الكل`); messages address the user directly in the imperative (`احفظ المستند…`).
- Western digits (0–9) in fixed labels (`تباعد أسطر 1.5`, `المستوى 3`), so they match the
  numbers the app fills in through placeholders. Decimal point kept as in the English source.
- Arabic comma `،` in lists (`A، B، C، …`) and guillemets «…» for quoted names
  (`البحث عن «{text}» في المستند`).
- Ribbon buttons with a `\n` break keep the break, split where the Arabic phrase breaks naturally.
- Names stay in Latin letters: WordCraft, Zotero, Mendeley, EndNote, SmartArt, WordArt, Discord,
  the Craft theme, file formats, paper sizes (Letter, Legal, Executive …), citation styles (APA,
  MLA, Chicago …), key names (Backspace, Tab, Page Up …) and the watermark texts that are inserted
  into the document as written (DRAFT, CONFIDENTIAL, DO NOT COPY, SAMPLE, ASAP, URGENT).
- No plural forms in the catalog format: counts use the singular after the number
  (`{n} سطر`, `{words} كلمة`), which reads acceptably for most counts; full Arabic plural rules
  (one, two, 3–10, 11+) need `@plural` support in WordCraft's catalog parser first.

## Terminology to review

| English | Arabic | Note |
|---|---|---|
| Document | مستند | |
| Paragraph / Section | فقرة / قسم | |
| Style / Theme | نمط / سمة | |
| Font | خط | |
| Ribbon / Tab (ribbon) | الشريط / علامة تبويب | |
| Home (tab) | الرئيسية | |
| Insert / Draw / Design / Layout | إدراج / الرسم / التصميم / التخطيط | |
| References / Mailings / Review / View | المراجع / المراسلات / مراجعة / عرض | |
| Select / Choose | تحديد / اختر | |
| Highlight / Mark | تمييز / تعليم | kept apart so they never mix |
| Track Changes / Markup | تتبع التغييرات / العلامات | |
| Comment / Reply / Resolve | تعليق / رد / حل | |
| Header / Footer | الرأس / التذييل | |
| Footnote / Endnote | حاشية سفلية / حاشية ختامية | |
| Table of Contents / Index | جدول المحتويات / الفهرس | |
| Caption / Cross-reference | تسمية توضيحية / إسناد ترافقي | |
| Citation / Bibliography / Source | استشهاد / قائمة المراجع / مصدر | |
| Bookmark / Hyperlink / Field | إشارة مرجعية / ارتباط تشعبي / حقل | |
| Mail Merge / Recipient | دمج المراسلات / مستلم | |
| Content control | عنصر تحكم محتوى | |
| Text box / Shape / Picture | مربع نص / شكل / صورة | |
| Indent / Hanging indent | مسافة بادئة / مسافة بادئة معلّقة | |
| Line spacing / Spacing | تباعد الأسطر / التباعد | |
| Margins / Orientation / Portrait / Landscape | الهوامش / الاتجاه / عمودي / أفقي | |
| Break (page, section, column) | فاصل | |
| Tab stop / Leader | علامة جدولة / الأحرف السابقة | |
| Bold / Italic / Underline / Strikethrough | غامق / مائل / تسطير / يتوسطه خط | |
| Superscript / Subscript / Small caps | مرتفع / منخفض / أحرف كبيرة صغيرة | |
| Proofing / Spelling / Grammar / Thesaurus | التدقيق / الإملاء / القواعد / المعجم | |
| Hyphenation | تقسيم الكلمات | |
| Drop cap / Watermark | حرف استهلالي / علامة مائية | |
| Outline (view) / Outline (shape border) | المخطط التفصيلي / حد خارجي | two different English senses |
| Pane / Dialog | جزء / نافذة | |
| Undo / Redo | تراجع / إعادة | |
| Overtype | الكتابة الفوقية | |
| Caret | مؤشر الكتابة | |
| Equation / Matrix / Integral / Summation | معادلة / مصفوفة / تكامل / مجموع | |

## Known gaps

- **New assembled lines** must follow the table above; the sites found by grepping `format!` and
  `join` over `tl!`/`t` in `ui-egui` were converted, but a later one written with `tl!` pieces
  will read wrong in Arabic until it is.
- **Interface mirroring.** The ribbon, panes, dialogs and rulers stay left to right (estimate in
  [target-app-parity › Localization](target-app-parity.md): 15–25 h). The open Hebrew PR (#20)
  covers the same work.
- **Arabic proofing.** No Arabic spelling or grammar; new documents keep the default language.
- **Kashida justification** in documents is still missing (Right-to-left and complex scripts row).
- The catalog is AI-assisted and awaits native-speaker review in the running app.

## Revision history

| Date | Change | Summary |
|---|---|---|
| 2026-10-17 | minor | Arabic catalog (2,616 entries), registry row, Arabic interface font fallback, labels in display order (`i18n::bidi`) |
| 2026-10-20 | minor | Review of #2677 (FredApps, CodeRabbit): `t_logical`/`tr_logical`/`fmt_logical` for stored, compared, OS-facing and automation text (untitled `مستند1`); `fmt` takes logical values and isolates them; assembled lines through `i18n::line`; 58 keys added upstream since |
| 2026-10-21 | trivial | Review of #2677 (FredApps, CodeRabbit): formatting-balloon and Style Inspector descriptions in reading order until drawn; command search in reading order; what stays unchanged for automation stated precisely |
