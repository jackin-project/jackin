# Independent review of the refreshed literature chapter

Reviewer: `/root/rtk_leanctx`, separately from the chapter24 author. The companion [review record](literature-independent-review.json) identifies the exact page and primary-source captures reviewed. This is a critical source/method review, not a rerun of any model experiment.

## Result

No blocking discrepancy was found in the reviewed high-risk study boundaries. The chapter distinguishes behavior-changing controlled agent experiments from fixed-trajectory accounting, preserves study-specific denominators, and explicitly withdraws the two incorrect historical routing citations. The TRACE-ROUTER to ORACLE revision is treated as a changed version and method rather than a timeless interchangeable title.

| Item independently checked | Finding |
|---|---|
| AgentDiet v2, sections 5–6 and Table4 | The 100-instance tuning set is excluded from the 200-instance Verified evaluation; Multi-SWE-bench Flash has 300 tasks. The four pass-rate pairs in chapter24 match Table4. The reported 21.1–35.9% total cost reduction includes reflection expense and cache effects; −1 to +2 percentage-point acceptance estimates do not establish noninferiority. |
| AgentDiet latency limitation | Section6.1.1 explicitly says latency was not quantitatively compared. The chapter makes no latency claim. This review suggested making the limitation explicit, and the author added it to the material-claim ledger. |
| TurnCost v1 methods and limitations | The 600 traces comprise the first 150 records from each of four scaffold/model shards. Tokenization and near-ideal prefix reuse are modeled, with a 0.1 relative cache-read price. The chapter correctly distinguishes these from provider receipts and does not claim compressed-counterfactual quality or removable-turn causation. |
| Earlier arXiv2504.11154 / 2408.11348 citations | The captured primary titles are SAR-to-RGB translation and Learning Flock particle filtering. They cannot support the earlier named router claims. The chapter explicitly withdraws that attribution. |
| TRACE-ROUTER v1 versus ORACLE v4 | The versioned primary abstracts and submission histories confirm the July24 TRACE title and October1 ORACLE revision, with changed authorship/method scope. The chapter does not attach old TRACE results to current ORACLE. |
| Current ORACLE scope | Task-specific verification, delayed feedback and DISC's expected peak-KV admission are described as a routing/serving intervention, outside the fixed-model central compression treatment. Full code/result reproduction is not claimed. |

The remaining paper survey was read as a complete chapter and cross-checked against the coordinator's earlier primary literature notes where applicable. It was not a second complete full-text audit of every cited paper. CAVEWOMAN paper/repository denominator distinctions, Notation Matters input/full treatments and APEC observational-versus-benchmark boundaries remain appropriately scoped. The chapter clearly marks metadata-only and inaccessible artifacts.

Primary sources used for the focused review: [AgentDiet v2](https://arxiv.org/html/2509.23586v2), [TurnCost v1](https://arxiv.org/html/2610.15093v1), [SAR-to-RGB](https://arxiv.org/abs/2504.11154v1), [Learning Flock](https://arxiv.org/abs/2408.11348v1), [TRACE-ROUTER v1](https://arxiv.org/abs/2607.22465v1), [ORACLE v4](https://arxiv.org/abs/2607.22465v4), [OrcaRouter v1](https://arxiv.org/abs/2605.30736v1). The review inspected the already-retrieved primary captures; it did not download the AgentDiet archive, retrieve the TurnCost Zenodo artifact, or execute an agent harness.
