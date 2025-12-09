#!/usr/bin/env bash
# regenerates the generated_assets allowlist in .gitignore based on
# files referenced from workflows/*.json
set -euo pipefail

cd "$(dirname "$0")/.."

begin="# BEGIN generated_assets allowlist"
end="# END generated_assets allowlist"

# find assets referenced from workflow definitions via /api/assets/ URLs
refs=$(find workflows -maxdepth 1 -name '*.json' ! -name '.temp_*' -print0 \
    | xargs -0 grep -hoE '/api/assets/[A-Za-z0-9._-]+' \
    | sed 's|/api/assets/|generated_assets/|' \
    | sort -u || true)
if [ -z "$refs" ]; then
    allow=""
else
    allow=$(echo "$refs" | sed 's|generated_assets/|!/generated_assets/|')
fi

tmp=$(mktemp)
awk -v b="$begin" -v e="$end" '
    $0==b {skip=1; next}
    $0==e {skip=0; next}
    !skip {print}
' .gitignore > "$tmp"

{
    cat "$tmp"
    echo "$begin"
    echo "/generated_assets/*"
    echo "$allow"
    echo "$end"
} > .gitignore

rm "$tmp"
count=0
[ -n "$allow" ] && count=$(echo "$allow" | wc -l)
echo "updated .gitignore with $count entries"
