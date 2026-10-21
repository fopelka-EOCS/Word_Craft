# Read Aloud

> **Last updated:** 2026-10-20 · **Change:** minor (voice roles; previously minor: text rules; previously minor: first version: how much Read Aloud reads at one time)

Read Aloud reads the document to you. It highlights the sentence that it reads, and the caret
follows that sentence.
This page tells you how to choose how much text Read Aloud reads at one time.
It also tells you what Read Aloud skips.
It also tells you how to give parts of the text to different voices.

All the settings are in **File › Options › Accessibility › Read Aloud**. The Read Aloud player
also has a link to them. Agents, the CLI and MCP use the `readAloud.settings` command. WordCraft
keeps the settings in `ui.json`.

## How much Read Aloud reads at one time

Set **Read at a time** (`"chunk"`):

- **One paragraph** (`paragraph`, the default). Read Aloud prepares one paragraph, reads it, and
  then goes to the next paragraph.
- **One page** (`page`). Read Aloud prepares the paragraphs that start on one page.
- **The whole document** (`document`). Read Aloud prepares all the sentences at the start.

The player shows the paragraph or page that it reads, for example "Paragraph 3 / 40".

**Next sentence** and **Previous sentence** go across paragraphs and pages.

## Text rules

The text rules change only the spoken text. The text on the page does not change, and the
highlight stays on the words as written. Read Aloud always removes invisible characters (zero-width
spaces, direction marks, soft hyphens).

| Setting (`text.…`) | Default | What it does |
|---|---|---|
| Web addresses (`urls`) | `domain` | `read` reads all of the address. `domain` reads only the site name (`https://www.example.com/a` becomes `example.com`). `skip` removes it. |
| Skip emoji (`skipEmoji`) | off | Removes emoji, skin tones and flags. |
| Skip symbols (`skipSymbols`) | on | Removes arrows, box and shape characters, bullets and icon-font characters. |
| Skip markup characters (`skipMarkup`) | off | Removes `*`, `#`, `_`, `~`, `\|`, `^`, `` ` ``, `<`, `>`, `=` and `\`. |
| Skip text in [square brackets] (`skipBrackets`) | off | Removes the brackets and the text in them. |
| Skip text in (parentheses) (`skipParentheses`) | off | Removes the parentheses and the text in them. |
| Skip repeated punctuation (`collapseRepeats`) | on | Removes four or more of the same mark in a row, for example `*****` or `-----`. |
| Characters to skip (`skipChars`) | empty | Removes each character in the list. |
| Say instead (`replacements`) | empty | Replaces text before the voice reads it. Write one per line: `& = and`. |
| Skip text that matches (`skipPatterns`) | empty | Removes text that matches a regular expression. Write one per line. |

Read Aloud applies the rules in this order: replacements, patterns, web addresses, brackets and
parentheses, emoji, symbols, markup, characters to skip, repeated punctuation.

Example: `{"text": {"replacements": [{"from": "Dr.", "to": "Doctor"}], "skipPatterns": ["\\[\\d+\\]"]}}`
reads "Dr. Lee [12]" as "Doctor Lee".

## Voice roles

Use voice roles to give parts of the text to different voices. The Narrator reads all text that
has no marker. The default roles are Voice A, Voice B and Voice C.

To change the voice, type the marker character and a role label before the text. The default
marker is `§`.

```
The door opened. §A Who is there? §B: Only me. §N The wind blew.
```

- The Narrator reads "The door opened."
- Voice A reads "Who is there?"
- Voice B reads "Only me."
- The Narrator reads "The wind blew."

Rules for markers:

- The label can have a colon after it (`§B:`). Read Aloud does not read the marker, the label, the
  colon or the spaces after them.
- `§N` and `§Narrator` go back to the Narrator.
- Labels are not case-sensitive (`§a` is the same as `§A`).
- If the label is not a role, Read Aloud reads the text as written. Thus `§5` in a legal text is
  read as written.
- **A marker changes the voice** (`voices.scope`): `paragraph` (the default) goes back to the
  Narrator at the end of the paragraph. `untilNext` keeps the voice until the next marker, also
  in the paragraphs that follow.

To set the roles, use the table in **File › Options › Accessibility › Read Aloud: voices**:

- **Label**: the text after the marker. Use letters, digits and `_`, 24 characters maximum.
- **System voice**: the voice of the role. Type a voice name (macOS: `say -v '?'` lists them;
  Linux and BSD: `espeak-ng --voices`; Windows: the installed voices). An empty field uses the
  default voice.
- **Speed**: the speed of the role, relative to the reading speed (0.5× to 2×).

Use **Add a voice** to add a role. You can have 26 roles and the Narrator. To use a different
marker, type one character in **Marker character**. The marker cannot be a letter, a digit, a
space, `_` or `:`.

## Command reference

`readAloud.settings` with no parameters returns all the settings. With parameters, it changes the
settings that you name. If one value is not correct, the command changes nothing and returns an
error. If Read Aloud is reading, it continues from the same sentence with the new settings.

```
wordcraft-cli run --template sample \
  --cmd 'readAloud.settings={"chunk":"page","voices":{"marker":"^"}}' \
  --cmd review.readAloud
```

`readAloud.status` also returns `chunk`, `chunkIndex`, `chunkCount` and `voice` (the role label of
the sentence that Read Aloud reads).

## Future work

These items are not in WordCraft yet:

- **Voices from quotation marks.** Read text in quotation marks with a dialogue voice, and read the
  text outside them with the Narrator. This needs rules for nested quotes and for the quote
  characters of each language.
- **Voices from the speaker.** Find the speaker of a quote from words near it ("said Anna", "Anna
  asked") and give the quote to the role for Anna.
- **Voices from pronouns.** Use pronouns near a quote ("she said", "they asked") and the last
  speaker that was named, to keep the same voice for the same character.
- **A character list.** Let the user map character names to roles, and keep the list with the
  document.
- **Word highlight.** Highlight each word as the voice reads it. This needs word timings from the
  voice engine.
- **Translations.** The new Options text has translations in Bulgarian, Croatian, Czech,
  Hungarian, Italian, Polish, Russian, Swedish, Turkish and Vietnamese. The other languages show
  it in English. The command labels have translations in all languages.

## Revision history

| Date | Change | Summary |
|---|---|---|
| 2026-10-20 | minor | Voice roles with a marker character |
| 2026-10-20 | minor | Text rules |
| 2026-10-20 | minor | First version: one paragraph, one page or the whole document at a time |
