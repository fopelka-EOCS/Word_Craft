# Read Aloud

> **Last updated:** 2026-10-20 · **Change:** minor (text rules; previously minor: first version: how much Read Aloud reads at one time)

Read Aloud reads the document to you. It highlights the sentence that it reads, and the caret
follows that sentence.
This page tells you how to choose how much text Read Aloud reads at one time.
It also tells you what Read Aloud skips.

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

## Command reference

`readAloud.settings` with no parameters returns all the settings. With parameters, it changes the
settings that you name. If one value is not correct, the command changes nothing and returns an
error. If Read Aloud is reading, it continues from the same sentence with the new settings.

```
wordcraft-cli run --template sample \
  --cmd 'readAloud.settings={"chunk":"page"}' \
  --cmd review.readAloud
```

`readAloud.status` also returns `chunk`, `chunkIndex` and `chunkCount`.

## Future work

These items are not in WordCraft yet:

- **Word highlight.** Highlight each word as the voice reads it. This needs word timings from the
  voice engine.
- **Translations.** The new Options text has translations in Bulgarian, Croatian, Czech,
  Hungarian, Italian, Polish, Russian, Swedish, Turkish and Vietnamese. The other languages show
  it in English. The command labels have translations in all languages.

## Revision history

| Date | Change | Summary |
|---|---|---|
| 2026-10-20 | minor | Text rules |
| 2026-10-20 | minor | First version: one paragraph, one page or the whole document at a time |
