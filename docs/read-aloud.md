# Read Aloud

> **Last updated:** 2026-10-20 · **Change:** minor (cloud voice; previously minor: neural voice on this computer, voice lists; previously minor: voice roles; previously minor: text rules; previously minor: first version: how much Read Aloud reads at one time)

Read Aloud reads the document to you. It highlights the sentence that it reads, and the caret
follows that sentence.
This page tells you how to choose the voice and how much text Read Aloud reads at one time.
It also tells you what Read Aloud skips.
It also tells you how to give parts of the text to different voices.

All the settings are in **File › Options › Accessibility › Read Aloud**. The Read Aloud player
also has a link to them. Agents, the CLI and MCP use the `readAloud.settings` command. WordCraft
keeps the settings in `ui.json`.

## Voice engines

Read Aloud has three voice engines. Select one engine.

| Engine | Setting | Where the speech is made | What you must install |
|---|---|---|---|
| System voice | `"engine": "system"` | On this computer | Nothing on macOS and Windows. On Linux and BSD, install `espeak-ng`. |
| Neural voice (Piper) | `"engine": "local"` | On this computer | Piper and one Piper voice model (`.onnx`). |
| Cloud voice | `"engine": "cloud"` | On a speech service | `curl` and an API key. |

The system voice is the default. The neural voice and the cloud voice sound more natural.

Note: Whisper is a speech-to-text model. It listens and writes text. It cannot speak, so Read
Aloud does not use it. The cloud engine uses the OpenAI speech endpoint, or any service that
uses the same request.

### Set up the neural voice (Piper)

1. Install Piper. Make sure that the `piper` program is on your `PATH`.
2. Download a Piper voice model. A model is an `.onnx` file and a `.onnx.json` file.
3. Open **File › Options › Accessibility**.
4. Select **Neural voice on this computer (Piper)**.
5. In **Voice model**, select the model from the list. If the model is not in the list, select
   **Browse…** in the list, or type the full path of the `.onnx` file.

The list shows the `.onnx` files in the folders of the models that you use, and in the usual
download folders (for example `~/.local/share/piper-voices`). Piper 1.8 installs with
`uv tool install piper-tts`. Its `python -m piper.download_voices` command downloads models.

Read Aloud gives the sentence to the program on stdin. The program writes a WAV file in a
private temporary folder that only your user can read. The default arguments are:

```
--model {voice} --output_file {output} --length_scale {lengthScale}
```

You can use a different program, for example a script that starts a different neural voice.
Change **Program** and **Arguments** in File › Options. Read Aloud fills in these fields for
each sentence:

| Field | Value |
|---|---|
| `{voice}` | The voice of the role, or else the default voice model |
| `{output}` | The WAV file that the program must write |
| `{rate}` | The speed, for example `1.25` |
| `{lengthScale}` | The length of the speech, `1 / speed` (Piper uses this) |

### Set up the cloud voice

CAUTION: The cloud voice sends the text that Read Aloud reads to the service at the address you
set. Do not use the cloud voice for text that must stay on this computer.

1. Put your API key in an environment variable. The default name is `OPENAI_API_KEY`.
2. Start WordCraft from an environment that has this variable.
3. Open **File › Options › Accessibility**.
4. Select **Cloud voice (sends the text to a service)**.
5. If necessary, change **Address**, **Model**, **Voice** and **API key variable**.

The pane shows if the API key is set. WordCraft never saves the API key. It saves only the name
of the variable. The address must start with `https://`. For a server on this computer, the
address can also start with `http://localhost` or `http://127.0.0.1`.

WordCraft sends the request with the system `curl` program. The API key goes to `curl` on stdin.
It is not on the command line.

### When a voice fails

If the neural voice or the cloud voice cannot make a sentence, the system voice reads that
sentence. The player shows the reason, for example a missing program or a missing API key.
Read Aloud stops only if the system voice also fails.

## How much Read Aloud reads at one time

Set **Read at a time** (`"chunk"`):

- **One paragraph** (`paragraph`, the default). Read Aloud prepares one paragraph, reads it, and
  then goes to the next paragraph.
- **One page** (`page`). Read Aloud prepares the paragraphs that start on one page.
- **The whole document** (`document`). Read Aloud prepares all the sentences at the start.

The neural voice and the cloud voice render all the sentences of the paragraph or page before
the player gets to them. With the whole document, they render three sentences ahead.
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
- **System voice**, **Neural voice**, **Cloud voice**: the voice for each engine. Select a voice
  from the list next to the field, or type a voice name or a path in the field.
  - The system list shows the voices of the speech program (`say -v '?'` on macOS,
    `espeak-ng --voices` on Linux and BSD, the installed voices on Windows).
  - The neural list shows the Piper models that WordCraft finds. Select **Browse…** to select a
    model file in a different folder.
  - The cloud list shows the voices of the OpenAI speech service, such as `nova`.
  - **Default voice** clears the field. An empty field uses the default voice of the engine.
- Select **Find voices again** after you install a voice.
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

For safety, `readAloud.settings` cannot change `local.program`, `local.args`, `cloud.endpoint` or
`cloud.apiKeyEnv`, and it cannot switch the cloud voice on. These settings choose the program that
WordCraft runs and where the text and the API key go. Change them in **File › Options ›
Accessibility**. A new neural voice path from the command can have only letters, digits and
`. _ / \ : -`, because the program arguments can be a shell command. The command accepts the same
values that it returned, so an agent can send the settings back.

`readAloud.voices` returns the voices in the lists: `system`, `local`, `cloud` and
`cloudModels`, each a list of `{"id", "label"}`. With `{"refresh": true}`, it asks the speech
program again.

`readAloud.status` also returns `engine`, `chunk`, `chunkIndex`, `chunkCount` and `voice` (the role
label of the sentence that Read Aloud reads).

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
| 2026-10-20 | minor | Cloud voice (OpenAI-compatible `/v1/audio/speech` through `curl`); a command can't change its address or key variable, or switch it on |
| 2026-10-20 | minor | Neural voice on this computer (Piper or another program), voice lists, `readAloud.voices`; a command can't change the program |
| 2026-10-20 | minor | Voice roles with a marker character |
| 2026-10-20 | minor | Text rules |
| 2026-10-20 | minor | First version: one paragraph, one page or the whole document at a time |
