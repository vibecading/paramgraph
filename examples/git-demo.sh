#!/usr/bin/env bash
# Git for building models, in two minutes. Run from Git Bash:
#   bash examples/git-demo.sh [path/to/pg.exe]
# It builds a throwaway repository next to this script (examples/git-demo-repo),
# makes two branches edit the same house, and merges them twice: with plain
# git and with pg as the merge driver.
set -e
HERE="$(cd "$(dirname "$0")" && pwd)"
PG="$(command -v "${1:-pg}" 2>/dev/null || true)"
[ -n "$PG" ] || PG="$HERE/../target/release/pg.exe"
[ -x "$PG" ] || { echo "build first: cargo build --release, or pass the path to pg.exe"; exit 2; }
REPO="$HERE/git-demo-repo"
# scratch files go next to the repo, in examples/
rm -rf "$REPO" && mkdir -p "$REPO" && cd "$REPO"
git init -q -b main
git config user.name "demo" && git config user.email "demo@example.invalid"
cp "$HERE/farnsworth.pg" house.pg
git add house.pg && git commit -qm "house as surveyed"

echo "== an agent, on its own branch, sets the glass back"
git checkout -qb agent
sed -i 's/^param g_y    = 0.202  D/param g_y    = 0.450  A/' house.pg
git commit -qam "agent: glass set back 0.45 m"
git show main:house.pg > ../base.pg && "$PG" diff ../base.pg house.pg

echo; echo "== meanwhile on main, the bays grow"
git checkout -q main
sed -i 's/^param s      = 6.706  P/param s      = 6.900  P/' house.pg
git commit -qam "bays to 6.9 m"
git show HEAD~1:house.pg > ../base.pg && "$PG" diff ../base.pg house.pg | head -6

echo; echo "== plain git: the edits are on different lines, so it merges silently"
git checkout -qb plain
git merge -q agent -m "merge agent" && echo "merged, nobody looked at the glass wall"

echo; echo "== git with pg as the merge driver"
git checkout -q main
git config merge.paramgraph.name "paramgraph three-way merge"
git config merge.paramgraph.driver "\"$PG\" merge %O %A %B %A"
echo '*.pg merge=paramgraph' > .gitattributes && git add .gitattributes && git commit -qm "use pg to merge models"
git merge agent -m "merge agent" || true
echo; echo "== what the reviewer sees before accepting"
git show HEAD:house.pg > ../ours.pg
"$PG" diff ../ours.pg house.pg
"$PG" diff-obj ../ours.pg house.pg ../review.obj
echo "examples/review.obj: open in Rhino or Blender (groups added / removed / moved / unchanged)"
echo "accept with: git add house.pg && git commit    reject with: git merge --abort"
