#!/usr/bin/env bash
# Regenerates the module dependency graph in docs/architecture.md from the
# source. It replaces everything between the `<!-- deps:start -->` and
# `<!-- deps:end -->` markers. Running it twice gives no diff.
#
# An edge `a --> b` means module `a` names module `b` in non-test code:
# through a `crate::` path, a `super::` path, or a bare `child::` path to a
# module the file declares with `mod child;`. Test modules (`#[cfg(test)]`
# followed by `mod tests`) and `//` comments are ignored.
#
# It also checks the feature rule: a feature imports only its own modules and
# `src/shared/`. Violations are printed on stderr; the graph is still written.
#
# Usage: docs/update.sh   (from anywhere; it works from the repository root)

set -euo pipefail

cd "$(dirname "$0")/.."

doc=docs/architecture.md
start_marker='<!-- deps:start -->'
end_marker='<!-- deps:end -->'

for marker in "$start_marker" "$end_marker"; do
  count=$(grep -cF -- "$marker" "$doc" || true)
  if [ "$count" != 1 ]; then
    echo "update.sh: expected exactly one '$marker' in $doc, found $count" >&2
    exit 1
  fi
done

# src/notify/terminal/mod.rs -> notify::terminal; src/main.rs -> main
module_of() {
  local path=${1#src/}
  path=${path%.rs}
  path=${path%/mod}
  printf '%s\n' "${path//\//::}"
}

files=$(rg --files src -g '*.rs' | LC_ALL=C sort)
modules=$(for file in $files; do module_of "$file"; done | LC_ALL=C sort)

# The non-test, comment-free source of one file.
production_source() {
  awk '
    $0 == "#[cfg(test)]" { pending = 1; next }
    pending && /^mod tests/ { exit }
    pending { print "#[cfg(test)]"; pending = 0 }
    { print }
  ' "$1" | sed 's://.*$::'
}

# The longest known module that `a::b::c` starts with, or nothing.
resolve() {
  local path=$1
  while :; do
    if grep -qxF -- "$path" <<< "$modules"; then
      printf '%s\n' "$path"
      return
    fi
    case $path in
      *::*) path=${path%::*} ;;
      *) return ;;
    esac
  done
}

edges=$(
  for file in $files; do
    from=$(module_of "$file")
    source=$(production_source "$file")

    # crate::a::b::c paths
    printf '%s\n' "$source" |
      grep -oE 'crate::[a-z_][a-z0-9_]*(::[a-z_][a-z0-9_]*)*' |
      sed 's/^crate:://' |
      while read -r path; do
        to=$(resolve "$path")
        [ -n "$to" ] && [ "$to" != "$from" ] && printf '%s %s\n' "$from" "$to"
      done || true

    # super:: paths name the parent module
    if grep -qE '(^|[^a-zA-Z0-9_:])super::' <<< "$source"; then
      case $from in
        *::*) printf '%s %s\n' "$from" "${from%::*}" ;;
      esac
    fi

    # bare child:: paths to modules this file declares
    children=$(printf '%s\n' "$source" |
      sed -nE 's/^[[:space:]]*(pub(\([a-z]+\))?[[:space:]]+)?mod[[:space:]]+([a-z_][a-z0-9_]*);.*/\3/p')
    for child in $children; do
      if [ "$from" = main ]; then target=$child; else target="$from::$child"; fi
      if grep -qE "(^|[^a-zA-Z0-9_:])$child::" <<< "$source"; then
        printf '%s %s\n' "$from" "$target"
      fi
    done
  done | LC_ALL=C sort -u
)

# Feature rule: a feature names only its own modules and shared.
printf '%s\n' "$edges" | while read -r from to; do
  [ -n "$from" ] || continue
  from_top=${from%%::*}
  to_top=${to%%::*}
  if [ "$from_top" != main ] && [ "$from_top" != "$to_top" ] && [ "$to_top" != shared ]; then
    echo "update.sh: rule violation: $from imports $to" >&2
  fi
done

node_id() { printf '%s\n' "${1//::/_}"; }

graph=$(
  echo '```mermaid'
  echo 'flowchart LR'
  echo '  main["main.rs"]'
  for top in $(printf '%s\n' "$modules" | grep -vx main | sed 's/::.*//' | LC_ALL=C sort -u); do
    echo "  subgraph feature_$top[\"src/$top/\"]"
    printf '%s\n' "$modules" | grep -E "^$top(::|\$)" | while read -r module; do
      if [ "$module" = "$top" ]; then
        label="$top (mod.rs)"
      else
        label=${module#"$top"::}
      fi
      echo "    $(node_id "$module")[\"$label\"]"
    done
    echo '  end'
  done
  printf '%s\n' "$edges" | while read -r from to; do
    [ -n "$from" ] && echo "  $(node_id "$from") --> $(node_id "$to")"
  done
  echo '```'
)

tmp=$(mktemp "${TMPDIR:-/tmp}/update-deps.XXXXXX")
trap 'rm -f "$tmp"' EXIT

GRAPH="$graph" awk -v start="$start_marker" -v end="$end_marker" '
  $0 == start { print; print ENVIRON["GRAPH"]; skipping = 1; next }
  $0 == end { skipping = 0 }
  !skipping { print }
' "$doc" > "$tmp"

if ! cmp -s "$tmp" "$doc"; then
  cat "$tmp" > "$doc"
  echo "update.sh: updated the dependency graph in $doc"
fi
