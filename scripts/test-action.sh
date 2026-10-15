#!/usr/bin/env bash
# Tests the GitHub Action's logic in scripts/action.sh, without installing anything. CI runs it
# with /bin/bash, which is bash 3.2 on macOS: `bash scripts/test-action.sh`.
set -o pipefail
here=$(cd "$(dirname "$0")" && pwd)
# shellcheck source=scripts/action.sh
. "$here/action.sh"

failures=0
fail() {
  echo "FAIL: $*"
  failures=$((failures + 1))
}

tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
printf '[workspace.package]\nversion = "9.8.7"\n' > "$tmp/Cargo.toml"

# version input, action ref, expected specifier
while IFS='|' read -r version ref want; do
  got=$(resolve_spec "$version" "$ref" "$tmp/Cargo.toml" 2>/dev/null) || fail "resolve_spec '$version' '$ref' failed"
  [[ "$got" == "$want" ]] || fail "resolve_spec '$version' '$ref': got '$got', want '$want'"
done <<'EOF'
|v2|==2.*
|v2.1|==2.1.*
|v2.1.0|==2.1.0
|0123456789abcdef0123456789abcdef01234567|==9.8.7
|main|==9.8.7
||==9.8.7
|v2.1.0-rc.1|==9.8.7
latest|v2|
2.1.0|v2|==2.1.0
2.*|0123456|==2.*
2.1.*|main|==2.1.*
EOF
for version in '>=2' '2.1' '2' 'v2.1.0' '2.1.0 ' '==2.1.0' 'latest;'; do
  resolve_spec "$version" v2 "$tmp/Cargo.toml" > /dev/null 2>&1 && fail "resolve_spec accepted '$version'"
done
resolve_spec "" main "$tmp/missing.toml" > /dev/null 2>&1 && fail "resolve_spec passed without a Cargo.toml"

# The default list: tracked files the hook would check, at any depth, without symlinks.
repo="$tmp/repo"
mkdir -p "$repo/nested/deep" "$repo/sub"
(
  cd "$repo" || exit 1
  git init -q .
  echo '{}' > a.json
  echo '{}' > nested/deep/b.jsonc
  echo '{}' > sub/.babelrc
  echo 'x' > notes.txt
  ln -s a.json link.json
  echo '{}' > untracked.json
  git add a.json nested sub notes.txt link.json
)
want=$'a.json\nnested/deep/b.jsonc\nsub/.babelrc'
got=$(cd "$repo" && list_files "" python3 && printf '%s\n' "${files[@]}")
[[ "$got" == "$want" ]] || fail "default list: got [$got], want [$want]"

# The files input: `**` reaches any depth (bash 3.2 has no globstar), a name that matches
# nothing stays, and two spaces make no empty name.
got=$(cd "$repo" && list_files "**/*.json  **/b.jsonc missing*.json untracked.json" python3 && printf '%s\n' "${files[@]}")
want=$'a.json\nlink.json\nuntracked.json\nnested/deep/b.jsonc\nmissing*.json\nuntracked.json'
[[ "$got" == "$want" ]] || fail "files input: got [$got], want [$want]"

# SARIF: the output must be a SARIF 2.1.0 document, whatever the exit code says.
fake() {
  printf '#!/usr/bin/env bash\n%s\n' "$1" > "$tmp/jsonvet"
  chmod +x "$tmp/jsonvet"
}
fake 'echo "{\"version\": \"2.1.0\", \"runs\": []}"; exit 1'
write_sarif "$tmp/jsonvet" python3 "$tmp/out.sarif" -- a.json 2>/dev/null || fail "SARIF with problems was rejected"
fake 'echo "{\"version\": \"2.1.0\", \"runs\": []}"'
write_sarif "$tmp/jsonvet" python3 "$tmp/out.sarif" -- a.json 2>/dev/null || fail "clean SARIF was rejected"
# jsonvet 2.0 takes `--format sarif` for two file names it cannot read.
fake 'echo "--format: cannot read"; exit 1'
write_sarif "$tmp/jsonvet" python3 "$tmp/out.sarif" -- a.json 2>/dev/null && fail "text output passed as SARIF"
fake 'echo "[1]"; exit 1'
write_sarif "$tmp/jsonvet" python3 "$tmp/out.sarif" -- a.json 2>/dev/null && fail "a JSON array passed as SARIF"
fake 'echo "{\"version\": \"2.1.0\"}"; exit 2'
write_sarif "$tmp/jsonvet" python3 "$tmp/out.sarif" -- a.json 2>/dev/null && fail "exit 2 passed"
write_sarif "$tmp/jsonvet" python3 "$tmp/no/such/dir/out.sarif" -- a.json 2>/dev/null && fail "an unwritable path passed"

if [[ $failures -gt 0 ]]; then
  echo "$failures failed"
  exit 1
fi
echo "all action tests passed (bash $BASH_VERSION)"
