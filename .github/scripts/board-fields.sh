#!/usr/bin/env bash
# Applies the board fields a work item declares in its body header to the project board.
# Runs in Actions, where GraphQL is available (cloud agent sessions block it).
#
#   board-fields.sh <issue-node-id>      body in $ISSUE_BODY
#
# The header is one HTML comment in the body:
#   <!-- board: gate=domain; eligible=yes; priority=P2 -->
# Keys: gate -> Gate class, eligible -> Agent-eligible, priority -> Priority. Values match
# the board's option names case-insensitively. Unknown keys and values are errors, so a
# typo fails the run instead of silently leaving a field unset. Status moves to Ready when
# the body carries a tech spec and the header a gate (board-lib.sh), and to Backlog when
# the item has none; it never moves back, and every later Status stays with board-sync.sh.
# Three requests per card at most: project, add-and-read, one batched update.
#
# Needs GH_TOKEN (classic PAT with repo + project scopes).
set -euo pipefail
# shellcheck source=board-lib.sh
source "$(dirname "$0")/board-lib.sh"

owner=WrackedFella
number=1
node=${1:?usage: board-fields.sh <issue-node-id>}

header=$(grep -m1 -oP '<!--\s*board:\K[^>]*(?=-->)' <<<"${ISSUE_BODY:-}" || true)
[[ -n $header ]] || { echo "no board header; nothing to do"; exit 0; }

declare -A want=()
IFS=';' read -ra pairs <<<"$header"
for pair in "${pairs[@]}"; do
  pair=${pair//[$'\t\r\n']/ }
  [[ -z ${pair//[[:space:]]/} ]] && continue
  key=$(sed -E 's/^[[:space:]]*([^=[:space:]]+)[[:space:]]*=.*/\1/' <<<"$pair")
  val=$(sed -E 's/^[^=]*=[[:space:]]*(.*[^[:space:]])[[:space:]]*$/\1/' <<<"$pair")
  case $key in
    gate) want["Gate class"]=$val ;;
    eligible) want["Agent-eligible"]=$val ;;
    priority) want["Priority"]=$val ;;
    *) echo "board-fields: unknown key '$key'" >&2; exit 1 ;;
  esac
done

proj=$(gh api graphql -f o="$owner" -F n="$number" -f query='
  query($o:String!,$n:Int!){ user(login:$o){ projectV2(number:$n){
    id fields(first:50){ nodes{ ... on ProjectV2FieldCommon{ id name }
      ... on ProjectV2SingleSelectField{ options{ id name } } } } } } }' \
  -q .data.user.projectV2)
pid=$(jq -r .id <<<"$proj")

# Adding is idempotent and returns the item with its current values, so no second read.
item=$(gh api graphql -f c="$node" -f p="$pid" -f query='
  mutation($c:ID!,$p:ID!){ addProjectV2ItemById(input:{projectId:$p,contentId:$c}){ item{ id
    fieldValues(first:50){ nodes{ ... on ProjectV2ItemFieldSingleSelectValue{
      name field{ ... on ProjectV2FieldCommon{ name } } } } } } } }' \
  -q .data.addProjectV2ItemById.item)
iid=$(jq -r .id <<<"$item")
current() { jq -r --arg f "$1" '[.fieldValues.nodes[] | select(.field.name==$f) | .name][0] // ""' <<<"$item"; }

status=$(current Status)
if [[ -v want["Gate class"] ]] && (($(rank "$status") < $(rank Ready))) \
  && grep -qE "$spec_heading_re" <<<"${ISSUE_BODY:-}"; then
  want["Status"]=Ready
elif [[ -z $status ]]; then
  want["Status"]=Backlog
fi

mutation='mutation($p:ID!,$i:ID!'
vars=()
body=''
n=0
for field in "${!want[@]}"; do
  option=${want[$field]}
  fid=$(jq -r --arg f "$field" '.fields.nodes[] | select(.name==$f) | .id' <<<"$proj")
  [[ -n $fid ]] || { echo "board-fields: no field '$field' on the board" >&2; exit 1; }
  oid=$(jq -r --arg f "$field" --arg o "$option" '
    .fields.nodes[] | select(.name==$f) | .options[]? | select(.name|ascii_downcase==($o|ascii_downcase)) | .id' <<<"$proj")
  [[ -n $oid ]] || { echo "board-fields: field '$field' has no option '$option'" >&2; exit 1; }
  name=$(jq -r --arg f "$field" --arg o "$option" '
    .fields.nodes[] | select(.name==$f) | .options[]? | select(.name|ascii_downcase==($o|ascii_downcase)) | .name' <<<"$proj")
  if [[ $(current "$field") == "$name" ]]; then
    echo "$field: already '$name'"
    continue
  fi
  echo "$field: '$(current "$field")' -> '$name'"
  mutation+=",\$f$n:ID!,\$o$n:String!"
  body+=" u$n:updateProjectV2ItemFieldValue(input:{projectId:\$p,itemId:\$i,fieldId:\$f$n,value:{singleSelectOptionId:\$o$n}}){ clientMutationId }"
  vars+=(-f "f$n=$fid" -f "o$n=$oid")
  n=$((n + 1))
done

((n > 0)) || { echo "board unchanged"; exit 0; }
gh api graphql -f p="$pid" -f i="$iid" "${vars[@]}" -f query="$mutation){$body }" >/dev/null
