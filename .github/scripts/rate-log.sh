#!/usr/bin/env bash
# Appends the token's remaining core and GraphQL quota to the job summary. The
# rate_limit endpoint does not count against either limit.
#
#   rate-log.sh <label>
#
# Needs GH_TOKEN. Never fails the job.
{
  printf '%s: ' "${1:?usage: rate-log.sh <label>}"
  gh api rate_limit -q '.resources | "core \(.core.remaining)/\(.core.limit), graphql \(.graphql.remaining)/\(.graphql.limit), resets \(.graphql.reset | todate)"' 2>&1 || true
} | tee -a "${GITHUB_STEP_SUMMARY:-/dev/null}"
exit 0
