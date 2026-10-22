# Registro delle Personalizzazioni Locali di WordCraft (CHANGELOG_CUSTOM)

Questo file documenta dettagliatamente tutte le modifiche e le personalizzazioni apportate a WordCraft, sia quelle grafiche e di traduzione (Sessione 1), sia quelle comportamentali e di default (Sessione 2).

Serve come riferimento permanente affinché, in caso di aggiornamenti dal repository principale (`upstream`: `storytold/wordcraft`), sia possibile verificare, preservare o riapplicare le proprie modifiche locali qualora non siano ancora state integrate a monte.

---

## 1. Modifiche Comportamentali e di Default

### 1.1 Lettera Maiuscola Solo Dopo il Punto (`.`)
- **Comportamento precedente**: Ogni nuovo paragrafo (a capo), ogni elemento di elenco puntato/numerato e ogni cella di tabella convertivano automaticamente la prima lettera in maiuscolo.
- **Nuovo comportamento**: La maiuscola automatica si attiva **esclusivamente dopo i terminatori di frase (`.`, `!`, `?`)**. Quando si va a capo, dopo un punto e virgola (`;`), o all'inizio di una voce di elenco puntato/numerato, la lettera rimane **minuscola**.
- **File modificati**:
  - `crates/engine/src/cmd/tools.rs`:
    - Aggiunto `pub cap_paragraphs: bool` (default `false`) a `AutoCorrectPrefs`.
    - La valutazione delle maiuscole all'inizio della cella (`cap_cells`), all'inizio del paragrafo/dell'elenco (`cap_paragraphs`), e all'interno del paragrafo (`cap_sentences`) è ora completamente indipendente e modulare.
    - Funzione `starts_sentence`: riconosce `.`, `!`, e `?` come terminatori validi di frase, mantenendo l'eccezione delle abbreviazioni per i punti fermi.
  - `crates/engine/src/tests_typing.rs`:
    - Aggiunto test unitario `default_preferences_lowercase_start_and_standard_hyphen` che verifica il mantenimento del minuscolo all'inizio del documento, a capo, dopo `;`, negli elenchi puntati e numerati, e la maiuscola dopo `.`, `!`, `?`.

### 1.2 Trattino Standard `-` di Default
- **Comportamento precedente**: La sequenza `" - "` o `"--"` veniva automaticamente convertita in trattino lungo (en-dash `–` o em-dash `—`).
- **Nuovo comportamento**: Il trattino standard `-` rimane esattamente come digitato, senza alcuna conversione automatica.
- **File modificati**:
  - `crates/engine/src/cmd/tools.rs`:
    - Impostato `dashes: false` di default in `AutoCorrectPrefs::default()`.
  - `crates/engine/src/cmd/text.rs`:
    - `dash_autoformat` viene eseguito solo se `s.prefs.autocorrect.dashes` è esplicitamente abilitato.

### 1.3 Spaziatura Paragrafo negli Elenchi a 6 pt
- **Comportamento precedente**: Gli elenchi puntati e numerati (`ListParagraph`) collassavano la spaziatura tra elementi consecutivi a 0 pt (`contextual_spacing = true`).
- **Nuovo comportamento**: Gli elenchi applicano di default **6 pt di spaziatura dopo ciascun paragrafo** (`space_after: 6.0`) e disabilitano il collasso contestuale (`contextual_spacing: false`).
- **File modificati**:
  - `crates/doc/src/styles.rs`:
    - Stile `ListParagraph`: `space_after: Some(6.0)`, `contextual_spacing: Some(false)`.

### 1.4 Dimensione Font a 12 pt negli Elenchi e Fallback
- **Comportamento precedente**: Il font degli elenchi e alcuni fallback di parsing DOCX retrocedevano a 10 pt o 11 pt.
- **Nuovo comportamento**: Dimensione coerente a **12 pt** sia per lo stile di elenco sia per i fallback di risoluzione.
- **File modificati**:
  - `crates/doc/src/styles.rs`: `chr.size: Some(12.0)` in `ListParagraph`.
  - `crates/docx/src/read/mod.rs`: Fallback dimensione carattere impostato a 12.0 pt.
  - `crates/docbin/src/fmt.rs`: Fallback dimensione carattere impostato a 12.0 pt.
  - `crates/doc/src/resolve.rs`: Risoluzione fallback dimensione impostata a 12.0 pt.

### 1.5 Selezione Parola al Doppio Click Senza Spazio Finale
- **Comportamento precedente**: Il doppio click su una parola selezionava la parola più lo spazio successivo (`word `).
- **Nuovo comportamento**: Il doppio click seleziona **esclusivamente i caratteri della parola** (`word`), escludendo lo spazio successivo.
- **File modificati**:
  - `crates/doc/src/para.rs`:
    - In `Paragraph::word_at`, rimosso l'ampliamento della selezione sugli spazi successivi; restituisce `(a, b)` con i limiti esatti del token di parola.
  - `crates/ui-egui/src/canvas.rs`:
    - Aggiornati i test di drag e selezione per verificare che il doppio click selezioni esclusivamente la parola.

### 1.6 Apertura Diretta Documenti da Finder (Doppio Click su File `.docx`)
- **Comportamento precedente**: Facendo doppio click su un documento associato dal Finder di macOS, l'app si apriva mostrando la schermata di benvenuto / "Ripristina sessione" (splash/recovery), richiedendo un click aggiuntivo per raggiungere il file.
- **Nuovo comportamento**: L'app si apre **immediatamente sul documento aperto**, nascondendo la schermata di backstage/recovery.
- **File modificati**:
  - `apps/wordcraft/src/apple_events.rs`:
    - All'apertura del file da Apple Event (`file.open`), imposta `app.ui.backstage = false; app.recovery_seen = true;`.
  - `crates/ui-egui/src/lib.rs`:
    - Reso `pub recovery_seen: bool` nel contesto dell'app e vincolata la schermata di recupero a `&& self.session.path.is_none()`.

---

## 2. Modifiche Grafiche e Interfaccia Utente (UI Fluent / Word Style)

### 2.1 Finestre di Dialogo in Stile Moderno Windows 11 / Word Fluent
- Sfondo scuro semitrasparente (`rgba(0, 0, 0, 0.40)`) per isolare il dialogo attivo.
- Finestre con angoli arrotondati a 10px, bordo sottile, titolo in grassetto (13.5 pt) con pulsante di chiusura standard.
- Schede interne con indicatori orizzontali accentati (linea attiva in basso).
- Pulsanti di azione standardizzati (Pulsante "OK" accentato blu Office `#0f6cbd`, pulsante "Annulla" neutro con angoli arrotondati a 4px).
- **File modificati**:
  - `crates/ui-egui/src/dialogs.rs`

### 2.2 Icone della Barra Multifunzione (Ribbon)
- Icone grandi ingrandite a 35×35 px (da 32×32 px).
- Icone piccole ingrandite a 19×19 px (da 17×17 px).
- Icone dei pulsanti secondari ingrandite a 16×16 px.
- Voci di menu contestuale per "Comprimere / Espandere la barra multifunzione" e "Mostra / Nascondi barra di accesso rapido".
- **File modificati**:
  - `crates/ui-egui/src/widgets.rs`, `crates/ui-egui/src/ribbon.rs`

### 2.3 Traduzioni e Revisione Italiano
- Traduzione di nuove voci di menu in `crates/ui-egui/src/i18n/it.tsv`.

### 2.4 Palette Cromatica Ufficiale Word Fluent
- `crates/ui-egui/src/theme.rs`:
  - Colore accento principale: aggiornato al blu firma di Microsoft Word / Office Fluent (`#0F6CBD`, precedentemente `#3B5BDB`).
  - Sfondo area di lavoro (canvas): aggiornato al tono neutro caldo di Word (`#EDEBE9`, precedentemente `#E6E6E6`).
  - Sfondo barre (title bar e tab strip): allineati a `#F3F3F3`.
  - Bordi e separatori: aggiornati al tono Fluent `#E1DFDD`.
  - Colori di interazione: hover a `#F3F2F1`, pressed a `#EDEBE9`, selezione attiva a `#EBF3FC`.

### 2.5 Ombreggiatura Morbida del Foglio di Lavoro
- `crates/ui-egui/src/canvas.rs`:
  - Sostituito il vecchio offset secco di 1px con una vera ombra sfumata a multi-layer (elevazione Fluent):
    - Ombra morbida diffusa a 5px (alpha 0x0A)
    - Ombra intermedia a 2.5px (alpha 0x12)
    - Ombra direzionale vicina a 1px (alpha 0x1E)
    - Bordo nitido perimetrale a 1px sul foglio (`Color32::from_rgba_unmultiplied(0, 0, 0, 0x18)`).

### 2.6 Barra Multifunzione (Ribbon Card & Schede)
- `crates/ui-egui/src/ribbon.rs`:
  - Scheda attiva: barra inferiore blu `#0F6CBD` a pillola (3px di altezza, raggio 1.5px).
  - Raggio di curvatura della card Ribbon: impostato a 8px per l'effetto contenitore moderno Fluent.

### 2.7 Titolo Finestra, Toggle AutoSave e Barra di Ricerca
- `crates/ui-egui/src/chrome.rs`:
  - Switch AutoSave: sagomato a pillola (`38x18px`) con indicatore circolare e testo `On` / `Off` integrato.
  - Barra di ricerca centrale: sagomata a pillola continua con raggio completo.

### 2.8 Icone Vettoriali e Stati Attivi in Stile Word
- `crates/ui-egui/src/widgets.rs` e `crates/ui-egui/src/theme.rs`:
  - Rimossa l'evidenziazione pesante a bordo blu attorno ai pulsanti attivi/selezionati (come allineamento, elenchi o grassetto). Ora lo stato attivo applica uno sfondo grigio morbido (`#E2E2E2`) identico a Word, senza bordi al neon.
- `crates/ui-egui/src/icons.rs`:
  - `painter` (Copia formato): ridisegnato come vero **pennello** inclinato con setole dorate, ghiera d'argento e manico in legno (sostituito il vecchio rullo).
  - `shading` (Sfondo paragrafo): ridisegnato come **secchiello** inclinato che versa una goccia (sostituito il vecchio rombo).
  - `borders` (Bordi): ridisegnato con la classica **finestra a 4 quadranti** (sostituita la griglia di puntini).
  - `copy` (Copia): fogli sovrapposti con contorno grigio standard (rimosso il bordo blu superfluo).
  - `indent` e `outdent` (Rientri): frecce in grigio scuro coerente (rimosse le frecce blu piene).
  - `charborder`, `enclose`, `subscript`, `superscript`: caratteri e indici in grigio scuro neutro.
  - `effects` (Effetti testo): lettera 'A' sagomata sobria senza bagliori blu invasivi.

---

## 3. Come Mantenere e Riapplicare le Modifiche con gli Aggiornamenti Upstream

Il fork GitHub personale è: `https://github.com/mattiianni/wordcraft`
Il branch di lavoro dedicato alle personalizzazioni è: `feat/italian-translation-and-ui-improvements`

### Scenario A: Aggiornare il branch da upstream mantenendo le modifiche (Rebase)
Quando l'autore principale di WordCraft (`upstream/main`) pubblica nuovi commit:

```bash
# 1. Assicurati che il remote upstream sia configurato
git remote add upstream https://github.com/storytold/wordcraft.git 2>/dev/null || true

# 2. Scarica gli aggiornamenti
git fetch upstream main

# 3. Riapplica i tuoi commit in cima alle ultime modifiche di WordCraft
git checkout feat/italian-translation-and-ui-improvements
git rebase upstream/main

# 4. In caso di conflitti, risolvili nei singoli file, poi:
# git add <file-risolto>
# git rebase --continue
```

### Scenario B: Salvare e riapplicare le personalizzazioni come Patch autonoma
È possibile esportare tutte le personalizzazioni in un singolo file di patch:

```bash
# Per creare la patch di tutte le differenze rispetto a upstream/main:
git diff upstream/main feat/italian-translation-and-ui-improvements > custom_wordcraft.patch

# Per riapplicare la patch su una cartella appena clonata o aggiornata:
git apply custom_wordcraft.patch
```

### Scenario C: Ricompilare e aggiornare l'app in `/Applications/`
Dopo qualsiasi aggiornamento o modifica al codice:

```bash
# 1. Compila la release ottimizzata
cargo build --release -p wordcraft

# 2. Copia l'eseguibile nel bundle dell'applicazione
/bin/cp -f target/release/wordcraft /Applications/WordCraft.app/Contents/MacOS/WordCraft

# 3. Firma il bundle per macOS
codesign --force --deep --sign - /Applications/WordCraft.app

# 4. Libera lo spazio su disco (rimuove ~10-15 GB di file temporanei intermedi)
cargo clean
```
