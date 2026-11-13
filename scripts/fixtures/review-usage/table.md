| Step | Round | Tier | Verdict | Model | Input | Output | Cache read | Cache write | Turns | Duration | Cost (API rate) |
|---|---|---|---|---|--:|--:|--:|--:|--:|--:|--:|
| review | 1 | full | reject / fix | claude-opus-5-5 | 1200 | 18000 | 800000 | 70000 | 41 | 412s | $2.1 |
| review | 1 | full | reject / fix | claude-fable-5-2 | 300 | 6000 | 110000 | 18000 |  |  | $1.11 |
| fix | 1 | full |  | claude-sonnet-5-5 | ? | ? | ? | ? | 28 | 139s | ? |
| fix | 1 | full |  | unsplit (no per-model counts) | 40 | 5000 | 300000 | 20000 |  |  | ? |
