#!/bin/sh
# Opens, or comments on, the one open issue labelled `nightly-failure` with
# a link to the failed nightly run (docs/compiler/release.md §8: a red
# nightly blocks the next branch cut). Run by nightly.yml's `report` job
# with GH_TOKEN set; needs `gh`.
#
#   scripts/ci/nightly-issue.sh https://github.com/<owner>/<repo>/actions/runs/<id>
set -eu
run="$1"
body="The nightly failed: $run. The next branch cut waits for a green nightly (docs/compiler/release.md §8)."
issue="$(gh issue list --label nightly-failure --state open --limit 1 --json number --jq '.[0].number // empty')"
if [ -n "$issue" ]; then
    gh issue comment "$issue" --body "$body"
else
    gh label create nightly-failure --color B60205 --description "The nightly failed" 2>/dev/null || true
    gh issue create --title "The nightly is red" --label nightly-failure --body "$body"
fi
