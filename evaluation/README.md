# AI Quality Evaluation

`cases.jsonl` is the versioned retrieval contract for code, terminal, browser,
small-font, multilingual, temporal, duplicate-frame, and citation scenarios.
Evaluation screenshots and OCR references must be synthetic or explicitly
sanitized before they are committed.

Export one JSON object per query with an `id` and ordered `retrieved_frames`,
then run:

```bash
python evaluation/evaluate.py evaluation/results.jsonl
```

Model or retrieval changes should improve Recall@10 and MRR without regressing
OCR confidence coverage, p95 latency, peak memory, or citation correctness.
