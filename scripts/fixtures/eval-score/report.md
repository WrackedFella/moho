### Replays

| Card | Class | Run | Gate | Steps missing | Reviews | Cause | Blockers | Ref tests | Peak context | Turns | Min |
|---|---|--:|---|---|---|---|---|--:|--:|--:|--:|
| DOM-1 | domain | 1 | pass | none | reject/human | conflict | B4* B1 | 3/4 | 151k | 41 | 30 |
| DOM-1 | domain | 2 | pass | none | reject/fix, approve/none | defect | B2 | 3/4 | 151k | 41 | 30 |
| DOM-2 | domain | 1 | none | none | - | limited | - | 3/4 | 151k | 41 | 30 |
| DOM-2 | domain | 2 | pass | none | - | paused | - | 3/4 | 151k | 41 | 30 |
| DOM-3 | domain | 1 | fail | none | approve/none | gate-fail | - | 3/4 | 151k | 41 | 30 |
| GLUE-1 | glue | 1 | pass | none | approve/none | pass | - | 3/4 | 151k | 41 | 30 |
| GLUE-1 | glue | 2 | pass | none | approve/none | pass | - | 3/4 | 151k | 41 | 30 |

`*` a needs-decision blocker. Cause uses the first review of the head.

### Summary

- Scored replays: 6 (1 cut off by the usage limit, not scored).
- Pass: 2 of 6; glue 2 of 2, domain 0 of 4.
- By cause: conflict 1, defect 1, gate-fail 1, pass 2, paused 1.
- Orchestrator pass (conflicts and pauses left out): 2 of 4.
- Pass on every run of a card: 1 of 2 cards.
- Same cause on every run of a card: 1 of 2 cards.
- Review flip rate (same head reviewed more than once, verdicts differ): 1 of 1 heads.

### Tokens per model (replays only)

| Model | Input | Output | Cache read | Cache write |
|---|--:|--:|--:|--:|
| claude-sonnet-5-5 | 70 | 14000 | 21000000 | 700000 |
