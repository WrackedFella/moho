#### Run record: #7 (domain), devflow f20ec3e45100

success, 9 turns, 120s, cost $1.5 at API rates. Tokens per model, never summed. Step output is an estimate (the run total shared by size of what each step wrote); other counts are exact:

| Step | Calls | Model | Input | Output (est.) | Cache read | Cache write | Files edited | Test files |
|---|--:|---|--:|--:|--:|--:|--:|--:|
| devflow:builder (tests) | 1 | claude-sonnet-5-5 | 20 | 180 | 2000 | 100 | 2 | 2 |
| orchestrator | 3 | claude-sonnet-5-5 | 20 | 230 | 7500 | 500 | 0 | 0 |
| skill devflow:wiki | 1 | claude-haiku-5-5 | 3 | 30 | 300 | 0 | 0 | 0 |

Mutants: 9 caught, 1 missed, 2 unviable, 0 timeout.
