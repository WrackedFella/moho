#!/usr/bin/env bash
# Moves every board item of this repo forward to the Status its issue implies, catching
# what the event-driven jobs missed (items older than the jobs, failed runs). Forward
# only, like board-sync.sh: closed issue -> Done; open issue with an open PR that closes
# it -> In review; open issue that meets the Ready rule (board-lib.sh) -> Ready.
#
#   board-backfill.sh [--dry-run]
#
# Reads the whole board in pages of 100, then one request per item it moves.
# Needs GH_TOKEN (classic PAT with repo + project scopes) and GITHUB_REPOSITORY.
set -euo pipefail
# shellcheck source=board-lib.sh
source "$(dirname "$0")/board-lib.sh"

owner=WrackedFella
number=1
dry_run=false
[[ ${1:-} == --dry-run ]] && dry_run=true

proj=$(gh api graphql -f o="$owner" -F n="$number" -f query='
  query($o:String!,$n:Int!){ user(login:$o){ projectV2(number:$n){
    id field(name:"Status"){ ... on ProjectV2SingleSelectField{ id options{ id name } } } } } }' \
  -q .data.user.projectV2)
pid=$(jq -r .id <<<"$proj")
fid=$(jq -r .field.id <<<"$proj")

# One line per item, `|`-separated so an empty Status keeps its place: item id, issue
# number, current Status, target Status.
plan=$(gh api graphql --paginate -f o="$owner" -F n="$number" -f query='
  query($o:String!,$n:Int!,$endCursor:String){ user(login:$o){ projectV2(number:$n){
    items(first:100,after:$endCursor){ pageInfo{ hasNextPage endCursor } nodes{ id
      fieldValueByName(name:"Status"){ ... on ProjectV2ItemFieldSingleSelectValue{ name } }
      content{ ... on Issue{ number state body repository{ nameWithOwner }
        closedByPullRequestsReferences(first:5){ nodes{ state } } } } } } } } }' \
  -q '.data.user.projectV2.items.nodes[]' |
  jq -r --arg repo "$GITHUB_REPOSITORY" --arg spec "(?m)$spec_heading_re" --arg gate "$gate_header_re" '
    select(.content.repository.nameWithOwner? == $repo)
    | (.content.body // "") as $body
    | (if .content.state == "CLOSED" then "Done"
       elif any(.content.closedByPullRequestsReferences.nodes[]; .state == "OPEN") then "In review"
       elif ($body | test($spec)) and ($body | test($gate)) then "Ready"
       else empty end) as $target
    | [.id, (.content.number | tostring), (.fieldValueByName.name // ""), $target] | join("|")')

moved=0
while IFS='|' read -r iid n cur target; do
  [[ -n $iid ]] || continue
  (($(rank "$cur") < $(rank "$target"))) || continue
  echo "#$n: '${cur:-none}' -> '$target'"
  moved=$((moved + 1))
  $dry_run && continue
  oid=$(jq -r --arg t "$target" '.field.options[] | select(.name==$t) | .id' <<<"$proj")
  [[ -n $oid ]] || { echo "board-backfill: no Status option '$target'" >&2; exit 1; }
  gh api graphql -f p="$pid" -f i="$iid" -f f="$fid" -f o="$oid" -f query='
    mutation($p:ID!,$i:ID!,$f:ID!,$o:String!){ updateProjectV2ItemFieldValue(input:{
      projectId:$p,itemId:$i,fieldId:$f,value:{singleSelectOptionId:$o}}){ projectV2Item{ id } } }' >/dev/null
done <<<"$plan"
echo "$moved item(s) $($dry_run && echo would move || echo moved)"
