# Read Aloud

> **Last updated:** 2026-10-20 · **Change:** minor (first version: how much Read Aloud reads at one time)

Read Aloud reads the document to you. It highlights the sentence that it reads, and the caret
follows that sentence.
This page tells you how to choose how much text Read Aloud reads at one time.

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
| 2026-10-20 | minor | First version: one paragraph, one page or the whole document at a time |
