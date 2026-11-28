### domain: 1 runs

Median tokens per run and model (never summed across models or kinds):

| Model | Input | Output | Cache read | Cache write |
|---|--:|--:|--:|--:|
| claude-haiku-5-5 | 3 | 30 | 300 | 0 |
| claude-sonnet-5-5 | 40 | 410 | 9k | 600 |

Steps: runs that called the step, and of those the runs where it edited no file.

A cache miss is a call after the first of its step that wrote over 20k tokens to the cache; records from before the meter show `-`.

| Step | Runs | Left nothing | Median calls | Median context peak | Runs with a cache miss |
|---|--:|--:|--:|--:|--:|
| devflow:builder (tests) | 1 | 0 | 1 | 2k | 0 |
| orchestrator | 1 | - | 3 | 4k | 0 |
| skill devflow:wiki | 1 | 1 | 1 | 303 | 0 |

Mutation: 1 of 1 runs read a result; survivors in 1.

