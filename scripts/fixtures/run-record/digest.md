### domain: 1 runs

Median tokens per run and model (never summed across models or kinds):

| Model | Input | Output | Cache read | Cache write |
|---|--:|--:|--:|--:|
| claude-haiku-5-5 | 3 | 30 | 300 | 0 |
| claude-sonnet-5-5 | 40 | 410 | 9k | 600 |

Steps: runs that called the step, and of those the runs where it edited no file.

| Step | Runs | Left nothing | Median calls |
|---|--:|--:|--:|
| devflow:builder (tests) | 1 | 0 | 1 |
| orchestrator | 1 | - | 3 |
| skill devflow:wiki | 1 | 1 | 1 |

Mutation: 1 of 1 runs read a result; survivors in 1.

