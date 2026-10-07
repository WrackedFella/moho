#!/usr/bin/env bash
# Moves issues to a target board Status, forward only. Runs in Actions, where GraphQL
# is available (cloud agent sessions block it).
#
#   board-sync.sh <Status> <issue-number>...
#
# Needs GH_TOKEN (classic PAT with repo + project scopes) and GITHUB_REPOSITORY.
set -euo pipefail

owner=WrackedFella
number=1
target=${1:?usage: board-sync.sh <Status> <issue-number>...}
shift

rank() {
  case $1 in
    Backlog | "") echo 0 ;;
    Ready) echo 1 ;;
    "In progress") echo 2 ;;
    "In review") echo 3 ;;
    Done) echo 4 ;;
    *) echo 0 ;;
  esac
}

proj=$(gh api graphql -f o="$owner" -F n="$number" -f query='
  query($o:String!,$n:Int!){ user(login:$o){ projectV2(number:$n){
    id field(name:"Status"){ ... on ProjectV2SingleSelectField{ id options{ id name } } } } } }' \
  -q .data.user.projectV2)
pid=$(jq -r .id <<<"$proj")
fid=$(jq -r .field.id <<<"$proj")
oid=$(jq -r --arg t "$target" '.field.options[] | select(.name==$t) | .id' <<<"$proj")
[[ -n $oid ]] || { echo "board-sync: no Status option '$target'" >&2; exit 1; }

for n in "$@"; do
  cid=$(gh api "repos/$GITHUB_REPOSITORY/issues/$n" -q .node_id)
  iid=$(gh api graphql -f c="$cid" -f p="$pid" -f query='
    mutation($c:ID!,$p:ID!){ addProjectV2ItemById(input:{projectId:$p,contentId:$c}){ item{ id } } }' \
    -q .data.addProjectV2ItemById.item.id)
  cur=$(gh api graphql -f i="$iid" -f query='
    query($i:ID!){ node(id:$i){ ... on ProjectV2Item{ fieldValueByName(name:"Status"){
      ... on ProjectV2ItemFieldSingleSelectValue{ name } } } } }' \
    -q '.data.node.fieldValueByName.name // ""')
  if (($(rank "$cur") >= $(rank "$target"))); then
    echo "#$n: '$cur' already at or past '$target', unchanged"
    continue
  fi
  gh api graphql -f p="$pid" -f i="$iid" -f f="$fid" -f o="$oid" -f query='
    mutation($p:ID!,$i:ID!,$f:ID!,$o:String!){ updateProjectV2ItemFieldValue(input:{
      projectId:$p,itemId:$i,fieldId:$f,value:{singleSelectOptionId:$o}}){ projectV2Item{ id } } }' >/dev/null
  echo "#$n: '${cur:-none}' -> '$target'"
done
