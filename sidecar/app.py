"""Managed local inference service for ScreenSearch quality features."""

from __future__ import annotations

import json
import logging
import os
import threading
from functools import lru_cache
from io import BytesIO
from typing import Annotated, Literal

import numpy as np
import torch
from fastapi import Depends, FastAPI, File, Form, Header, HTTPException, UploadFile
from fastapi.responses import JSONResponse
from starlette.requests import Request
from pydantic import BaseModel, Field
from PIL import Image
from sentence_transformers import CrossEncoder, SentenceTransformer

EMBEDDING_MODEL = "Qwen/Qwen3-Embedding-0.6B"
RERANKER_MODEL = "Qwen/Qwen3-Reranker-0.6B"
MODEL_VERSION = "main"
EMBEDDING_DIMENSION = 1024
RETRIEVAL_INSTRUCTION = (
    "Given a screen-history search query, retrieve relevant timestamped OCR "
    "passages that answer the query."
)

app = FastAPI(title="ScreenSearch Quality Sidecar", version="1")
logger = logging.getLogger("screensearch.sidecar")


class EmbeddingRequest(BaseModel):
    model: str
    texts: list[str] = Field(min_length=1, max_length=64)
    task: Literal["query", "document"] = "document"


class EmbeddingResponse(BaseModel):
    embeddings: list[list[float]]
    model: str
    version: str
    dimension: int


class RerankRequest(BaseModel):
    model: str
    query: str
    documents: list[str] = Field(min_length=1, max_length=100)


class RerankResponse(BaseModel):
    scores: list[float]


class ChunkRequest(BaseModel):
    model: str
    text: str
    max_tokens: int = Field(default=512, ge=32, le=4096)
    overlap_tokens: int = Field(default=64, ge=0, le=1024)


class ChunkResponse(BaseModel):
    chunks: list[str]


class OcrLine(BaseModel):
    text: str
    confidence: float
    bbox: tuple[int, int, int, int]


class OcrResponse(BaseModel):
    provider: str = "ppocr-v5"
    language: str | None
    orientation_degrees: float | None = None
    lines: list[OcrLine]


class ModelPreparationRequest(BaseModel):
    components: list[Literal["ocr", "embeddings", "reranker"]] = Field(
        default_factory=lambda: ["ocr", "embeddings", "reranker"],
        min_length=1,
    )
    ocr_language: str = "en"


class ModelPreparationStatus(BaseModel):
    state: Literal["idle", "preparing", "ready", "error"]
    current_component: str | None = None
    ready_components: list[str] = Field(default_factory=list)
    error: str | None = None


_preparation_lock = threading.Lock()
_preparation_status = ModelPreparationStatus(state="idle")


def authorize(authorization: Annotated[str | None, Header()] = None) -> None:
    expected = os.getenv("SCREENSEARCH_AI_SIDECAR_TOKEN")
    if expected and authorization != f"Bearer {expected}":
        raise HTTPException(status_code=401, detail="invalid sidecar token")


def device() -> str:
    return "cuda" if torch.cuda.is_available() else "cpu"


@lru_cache(maxsize=1)
def embedding_model() -> SentenceTransformer:
    return SentenceTransformer(
        EMBEDDING_MODEL,
        device=device(),
        model_kwargs={"torch_dtype": "auto"},
    )


@lru_cache(maxsize=1)
def reranker_model() -> CrossEncoder:
    return CrossEncoder(
        RERANKER_MODEL,
        device=device(),
        max_length=8192,
        prompts={"screensearch": RETRIEVAL_INSTRUCTION},
        default_prompt_name="screensearch",
    )


@lru_cache(maxsize=8)
def ocr_model(language: str):
    from paddleocr import PaddleOCR

    return PaddleOCR(
        lang=language,
        ocr_version="PP-OCRv5",
        device="cpu",
        enable_mkldnn=False,
        use_doc_orientation_classify=True,
        use_doc_unwarping=False,
        use_textline_orientation=True,
    )


@app.exception_handler(Exception)
async def unhandled_error(_request: Request, error: Exception) -> JSONResponse:
    logger.exception("Unhandled quality sidecar request failure")
    return JSONResponse(
        status_code=500,
        content={"detail": f"{type(error).__name__}: {error}"},
    )


@app.get("/health", dependencies=[Depends(authorize)])
def health() -> dict[str, object]:
    return {
        "status": "ok",
        "device": device(),
        "embedding_model": EMBEDDING_MODEL,
        "reranker_model": RERANKER_MODEL,
        "embedding_dimension": EMBEDDING_DIMENSION,
    }


def _prepare_models(components: list[str], ocr_language: str) -> None:
    global _preparation_status

    ready: list[str] = []
    try:
        for component in components:
            with _preparation_lock:
                _preparation_status = ModelPreparationStatus(
                    state="preparing",
                    current_component=component,
                    ready_components=ready,
                )

            if component == "ocr":
                ocr_model(ocr_language)
            elif component == "embeddings":
                embedding_model()
            elif component == "reranker":
                reranker_model()
            ready.append(component)

        with _preparation_lock:
            _preparation_status = ModelPreparationStatus(
                state="ready",
                ready_components=ready,
            )
    except Exception as error:
        logger.exception(
            "Model preparation failed while initializing %s",
            _preparation_status.current_component,
        )
        with _preparation_lock:
            _preparation_status = ModelPreparationStatus(
                state="error",
                current_component=_preparation_status.current_component,
                ready_components=ready,
                error=str(error),
            )


@app.get(
    "/v1/models/status",
    response_model=ModelPreparationStatus,
    dependencies=[Depends(authorize)],
)
def model_preparation_status() -> ModelPreparationStatus:
    with _preparation_lock:
        return _preparation_status.model_copy(deep=True)


@app.post(
    "/v1/models/prepare",
    response_model=ModelPreparationStatus,
    dependencies=[Depends(authorize)],
)
def prepare_models(request: ModelPreparationRequest) -> ModelPreparationStatus:
    global _preparation_status

    with _preparation_lock:
        if _preparation_status.state == "preparing":
            return _preparation_status.model_copy(deep=True)
        _preparation_status = ModelPreparationStatus(
            state="preparing",
            current_component=request.components[0],
        )

    threading.Thread(
        target=_prepare_models,
        args=(request.components, request.ocr_language),
        daemon=True,
        name="model-preparation",
    ).start()
    return _preparation_status.model_copy(deep=True)


@app.post(
    "/v1/embeddings",
    response_model=EmbeddingResponse,
    dependencies=[Depends(authorize)],
)
def embeddings(request: EmbeddingRequest) -> EmbeddingResponse:
    if request.model != EMBEDDING_MODEL:
        raise HTTPException(status_code=400, detail="unsupported embedding model")

    kwargs: dict[str, object] = {
        "normalize_embeddings": True,
        "truncate_dim": EMBEDDING_DIMENSION,
        "convert_to_numpy": True,
        "show_progress_bar": False,
    }
    if request.task == "query":
        kwargs["prompt"] = f"Instruct: {RETRIEVAL_INSTRUCTION}\nQuery:"

    vectors = embedding_model().encode(request.texts, **kwargs)
    return EmbeddingResponse(
        embeddings=vectors.astype(np.float32).tolist(),
        model=EMBEDDING_MODEL,
        version=MODEL_VERSION,
        dimension=EMBEDDING_DIMENSION,
    )


@app.post(
    "/v1/chunk",
    response_model=ChunkResponse,
    dependencies=[Depends(authorize)],
)
def chunk(request: ChunkRequest) -> ChunkResponse:
    if request.model != EMBEDDING_MODEL:
        raise HTTPException(status_code=400, detail="unsupported embedding model")
    if request.overlap_tokens >= request.max_tokens:
        raise HTTPException(status_code=400, detail="overlap must be smaller than max_tokens")

    tokenizer = embedding_model().tokenizer
    token_ids = tokenizer.encode(request.text, add_special_tokens=False)
    step = request.max_tokens - request.overlap_tokens
    chunks = [
        tokenizer.decode(token_ids[start : start + request.max_tokens]).strip()
        for start in range(0, len(token_ids), step)
    ]
    return ChunkResponse(chunks=[value for value in chunks if value])


@app.post(
    "/v1/rerank",
    response_model=RerankResponse,
    dependencies=[Depends(authorize)],
)
def rerank(request: RerankRequest) -> RerankResponse:
    if request.model != RERANKER_MODEL:
        raise HTTPException(status_code=400, detail="unsupported reranker model")
    pairs = [(request.query, document) for document in request.documents]
    scores = reranker_model().predict(
        pairs,
        activation_fn=torch.nn.Sigmoid(),
        show_progress_bar=False,
    )
    return RerankResponse(scores=np.asarray(scores, dtype=np.float32).tolist())


@app.post(
    "/v1/ocr",
    response_model=OcrResponse,
    dependencies=[Depends(authorize)],
)
async def ocr(
    image: Annotated[UploadFile, File()],
    language: Annotated[str, Form()] = "en",
) -> OcrResponse:
    raw = await image.read()
    pixels = np.asarray(Image.open(BytesIO(raw)).convert("RGB")).copy()
    results = ocr_model(language).predict(pixels)
    lines: list[OcrLine] = []
    orientation: float | None = None

    for result in results:
        payload = getattr(result, "json", result)
        if isinstance(payload, str):
            payload = json.loads(payload)
        if callable(payload):
            payload = payload()
        if isinstance(payload, dict) and "res" in payload:
            payload = payload["res"]
        if not isinstance(payload, dict):
            continue

        texts = payload.get("rec_texts", [])
        scores = payload.get("rec_scores", [])
        boxes = payload.get("rec_boxes", [])
        preprocessor = payload.get("doc_preprocessor_res")
        if isinstance(preprocessor, dict):
            orientation = preprocessor.get("angle", orientation)
        for text, score, box in zip(texts, scores, boxes, strict=False):
            coordinates = np.asarray(box)
            if coordinates.shape == (4,):
                x1, y1, x2, y2 = [int(value) for value in coordinates]
            elif coordinates.ndim == 2 and coordinates.shape[1] == 2:
                x1 = int(coordinates[:, 0].min())
                y1 = int(coordinates[:, 1].min())
                x2 = int(coordinates[:, 0].max())
                y2 = int(coordinates[:, 1].max())
            else:
                logger.warning(
                    "Ignoring unsupported OCR box shape: %s", coordinates.shape
                )
                continue
            lines.append(
                OcrLine(
                    text=str(text),
                    confidence=float(score),
                    bbox=(x1, y1, max(0, x2 - x1), max(0, y2 - y1)),
                )
            )

    return OcrResponse(
        language=language,
        orientation_degrees=orientation,
        lines=lines,
    )


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="127.0.0.1", port=3132)
