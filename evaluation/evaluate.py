"""Compute retrieval metrics from ScreenSearch evaluation result JSONL."""

from __future__ import annotations

import argparse
import json
from pathlib import Path


def load_jsonl(path: Path) -> dict[str, dict]:
    return {
        item["id"]: item
        for line in path.read_text(encoding="utf-8").splitlines()
        if line.strip()
        for item in [json.loads(line)]
    }


def retrieval_metrics(cases: dict[str, dict], results: dict[str, dict], k: int) -> dict:
    recalls: list[float] = []
    reciprocal_ranks: list[float] = []
    for case_id, case in cases.items():
        relevant = set(case["relevant_frames"])
        retrieved = results.get(case_id, {}).get("retrieved_frames", [])[:k]
        hits = relevant.intersection(retrieved)
        recalls.append(len(hits) / len(relevant))
        rank = next(
            (index for index, frame in enumerate(retrieved, start=1) if frame in relevant),
            None,
        )
        reciprocal_ranks.append(0.0 if rank is None else 1.0 / rank)
    return {
        f"recall@{k}": sum(recalls) / len(recalls),
        "mrr": sum(reciprocal_ranks) / len(reciprocal_ranks),
        "cases": len(cases),
    }


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("results", type=Path)
    parser.add_argument("--cases", type=Path, default=Path(__file__).with_name("cases.jsonl"))
    parser.add_argument("-k", type=int, default=10)
    args = parser.parse_args()
    print(json.dumps(retrieval_metrics(load_jsonl(args.cases), load_jsonl(args.results), args.k), indent=2))


if __name__ == "__main__":
    main()
