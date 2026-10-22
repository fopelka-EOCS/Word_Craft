#!/usr/bin/env bash
set -e

# Sync Upstream with Custom Preserved Modifications
# WordCraft

REPO_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$REPO_DIR"

echo "=== [WordCraft] Verifica aggiornamenti upstream da origin/main ==="

# Assicuriamoci che i remoti siano configurati
git fetch origin main

NEW_COMMITS=$(git rev-list --count HEAD..origin/main)

if [ "$NEW_COMMITS" -eq 0 ]; then
    echo "Nessun nuovo aggiornamento dalla community. WordCraft è già allineato all'ultima versione di upstream!"
    exit 0
fi

echo "Trovati $NEW_COMMITS nuovi commit upstream."
echo "Avvio rebase per incorporare le migliorie della community preservando le personalizzazioni..."

CURRENT_BRANCH=$(git rev-parse --abbrev-ref HEAD)

# Tentativo di rebase
if git rebase origin/main; then
    echo "Rebase completato con successo senza conflitti!"
else
    echo "Attenzione: si sono verificati conflitti durante il rebase."
    echo "Risolvi i conflitti assicurandoti di mantenere le modifiche in CHANGELOG_CUSTOM.md, poi prosegui con 'git rebase --continue'."
    exit 1
fi

echo "Verifica dei test di digitazione e preferenze personalizzate..."
cargo test -p wordcraft-engine tests_typing::default_preferences_lowercase_start_and_standard_hyphen

echo "Compilazione dell'applicazione aggiornata..."
cargo build --release -p wordcraft

if [ -d "/Applications/WordCraft.app" ]; then
    echo "Installazione del nuovo eseguibile in /Applications/WordCraft.app..."
    /bin/cp -f target/release/wordcraft /Applications/WordCraft.app/Contents/MacOS/WordCraft
    xattr -cr /Applications/WordCraft.app
    codesign --force --deep --sign - /Applications/WordCraft.app 2>/dev/null || true
    echo "Applicazione aggiornata e firmata con successo in /Applications/WordCraft.app!"
fi

echo "Pulizia cartella target per risparmiare spazio su disco..."
cargo clean

echo "Aggiornamento del fork su GitHub..."
git push --force-with-lease fork "$CURRENT_BRANCH" || true

echo "=== [WordCraft] Aggiornamento completato con successo! ==="
