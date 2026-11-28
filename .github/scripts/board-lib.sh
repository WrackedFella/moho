#!/usr/bin/env bash
# Source this file: Status ordering and the Ready rule shared by the board scripts.

# A card is Ready once its body has a tech spec heading and a board header naming its
# gate class (_todo/_STANDARDS.md, card lifecycle). A Tech Lead publishes the spec only
# after the user approves it, so the heading is the approval. Both patterns suit grep -E
# and jq; jq needs a leading (?m) for ^ to match at each line.
spec_heading_re='^#{2,3}[[:space:]]*Tech spec'
gate_header_re='<!--[[:space:]]*board:[^>]*gate='

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
