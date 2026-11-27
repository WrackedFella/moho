#### Run record: #8 (glue), devflow f20ec3e45100

success, 5 turns, 1s, cost $1 at API rates. Tokens per model, never summed. Step output is an estimate (the run total shared by size of what each step wrote); other counts are exact:

| Step | Calls | Model | Input | Output (est.) | Cache read | Cache write | Files edited | Test files |
|---|--:|---|--:|--:|--:|--:|--:|--:|
| devflow:builder (tests) | 3 | claude-sonnet-5-5 | 60 | 93 | 50000 | 85500 | 0 | 0 |
| orchestrator | 2 | claude-sonnet-5-5 | 15 | 87 | 4000 | 500 | 0 | 0 |

Context and cache per step (tokens), and tool results as they entered context (calls, bytes). A miss is a call after the first of its step that wrote more than 20k tokens to the cache:

| Step | Context peak | Cache write 5m | Cache write 1h | Misses | Miss write | Tool results |
|---|--:|--:|--:|--:|--:|---|
| devflow:builder (tests) | 55020 | 85500 | 0 | 1 | 45000 | Bash 1x 100B, Read 1x 4000B, mcp__gitnexus__impact 1x 300B |
| orchestrator | 3005 | 0 | 500 | 0 | 0 | Agent 1x 15B, Bash 1x 70B |

Mutants: no mutation run read.
