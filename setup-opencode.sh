#!/usr/bin/env bash
set -euo pipefail

# Link OpenCode's global config locations back to this repo. Skills are not
# linked here because opencode.jsonc already points skills.paths at this repo.

REPO="$HOME/coding-config"
CONFIG_DIR="$HOME/.config/opencode"

CONFIG_SRC="$REPO/opencode.jsonc"
TOOLS_SRC="$REPO/tools"
ALIAS_FILE="$HOME/.alias"
LEGACY_ALIASES_FILE="$HOME/.aliases"
BASHRC_FILE="$HOME/.bashrc"

OPENCODE_ALIAS="alias ocode='OPENCODE_ENABLE_EXA=1 OPENCODE_EXPERIMENTAL_LSP_TOOL=1 OPENCODE_EXPERIMENTAL_SCOUT=1 OPENCODE_EXPERIMENTAL_PLAN_MODE=1 opencode'"
OCODE_ALIAS_PATTERN='^[[:space:]]*alias[[:space:]]+ocode='
ALIAS_LOAD_LINE='[ -f "$HOME/.alias" ] && . "$HOME/.alias"'
ALIAS_LOAD_PATTERN="(^[[:space:]]*(source|[.])[[:space:]]+[\"']?(~|[$]HOME)/[.]alias(es)?[\"']?[[:space:]]*$|^[[] -f \"[$]HOME/[.]alias(es)?\" []] && [.] \"[$]HOME/[.]alias(es)?\"$)"

require_path() {
  local path="$1"

  if [ ! -e "$path" ]; then
    printf 'error: expected %s to exist\n' "$path" >&2
    exit 1
  fi
}

link_path() {
  local src="$1"
  local dest="$2"

  require_path "$src"
  mkdir -p "$(dirname "$dest")"

  if [ -e "$dest" ] && [ ! -L "$dest" ]; then
    printf 'error: %s already exists and is not a symlink\n' "$dest" >&2
    printf 'Move it aside, then re-run this script.\n' >&2
    exit 1
  fi

  ln -sfnT "$src" "$dest"
  printf 'linked %s -> %s\n' "$dest" "$src"
}

migrate_legacy_alias_file() {
  if [ -f "$LEGACY_ALIASES_FILE" ] && [ ! -e "$ALIAS_FILE" ]; then
    mv "$LEGACY_ALIASES_FILE" "$ALIAS_FILE"
    printf 'renamed %s -> %s\n' "$LEGACY_ALIASES_FILE" "$ALIAS_FILE"
  fi
}

upsert_line() {
  local file="$1"
  local pattern="$2"
  local replacement="$3"
  local description="$4"
  local input="/dev/null"
  local tmp

  if [ -e "$file" ] && [ ! -f "$file" ]; then
    printf 'error: %s already exists and is not a file\n' "$file" >&2
    exit 1
  fi

  if [ -f "$file" ]; then
    input="$file"
  fi

  tmp="$(mktemp "$file.tmp.XXXXXX")"
  awk -v pattern="$pattern" -v replacement="$replacement" '
    $0 ~ pattern {
      if (!replaced) print replacement
      replaced = 1
      next
    }
    { print }
    END {
      if (!replaced) print replacement
    }
  ' "$input" > "$tmp"

  mv "$tmp" "$file"
  printf 'upserted %s in %s\n' "$description" "$file"
}

require_path "$REPO"

link_path "$CONFIG_SRC" "$CONFIG_DIR/opencode.json"

# OpenCode has no tools.paths config equivalent to skills.paths. Custom tools
# are discovered from .opencode/tools or ~/.config/opencode/tools.
link_path "$TOOLS_SRC" "$CONFIG_DIR/tools"

migrate_legacy_alias_file
upsert_line "$ALIAS_FILE" "$OCODE_ALIAS_PATTERN" "$OPENCODE_ALIAS" "ocode alias"
upsert_line "$BASHRC_FILE" "$ALIAS_LOAD_PATTERN" "$ALIAS_LOAD_LINE" ".alias loader"
