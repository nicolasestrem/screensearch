"""Managed local inference service for ScreenSearch quality features."""

from __future__ import annotations

import json
import logging
import os
import threading
from functools import lru_cache
from importlib.metadata import PackageNotFoundError, version
from io import BytesIO
from typing import Annotated, Literal

import numpy as np
import torch
from fastapi import Depends, FastAPI, File, Form, Header, HTTPException, UploadFile
from fastapi.concurrency import run_in_threadpool
from fastapi.responses import JSONResponse
from starlette.requests import Request
from pydantic import BaseModel, Field
from PIL import Image
from sentence_transformers import CrossEncoder, SentenceTransformer

EMBEDDING_MODEL = "Qwen/Qwen3-Embedding-0.6B"
RERANKER_MODEL = "Qwen/Qwen3-Reranker-0.6B"
MODEL_VERSION = "main"
EMBEDDING_DIMENSION = 1024
MAX_OCR_UPLOAD_BYTES = 20 * 1024 * 1024
MAX_OCR_REQUEST_BYTES = MAX_OCR_UPLOAD_BYTES + 1024 * 1024
MAX_OCR_PIXELS = 50_000_000
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
_embedding_model_lock = threading.Lock()
_reranker_model_lock = threading.Lock()
_ocr_model_lock = threading.Lock()


def _paddleocr_version() -> str:
    try:
        installed = version("paddleocr")
    except PackageNotFoundError as error:
        raise RuntimeError("PaddleOCR 3.x is required but is not installed") from error
    if installed.split(".", maxsplit=1)[0] != "3":
        raise RuntimeError(f"PaddleOCR 3.x is required, got {installed}")
    return installed


PADDLEOCR_VERSION = _paddleocr_version()


@app.middleware("http")
async def reject_oversized_ocr_request(request: Request, call_next):
    if request.url.path == "/v1/ocr":
        content_length = request.headers.get("content-length")
        if content_length is not None:
            try:
                request_bytes = int(content_length)
            except ValueError:
                return JSONResponse(
                    status_code=400,
                    content={"detail": "invalid Content-Length header"},
                )
            if request_bytes > MAX_OCR_REQUEST_BYTES:
                return JSONResponse(
                    status_code=413,
                    content={"detail": "OCR multipart request is too large"},
                )
    return await call_next(request)


def authorize(authorization: Annotated[str | None, Header()] = None) -> None:
    expected = os.getenv("SCREENSEARCH_AI_SIDECAR_TOKEN")
    if expected and authorization != f"Bearer {expected}":
        raise HTTPException(status_code=401, detail="invalid sidecar token")


def device() -> str:
    return "cuda" if torch.cuda.is_available() else "cpu"


def paddle_device() -> str:
    """Select the PaddleOCR device, preferring an available CUDA GPU.

    Returns "gpu" only when Paddle was built with CUDA AND a usable device is
    present, so a GPU-enabled build still falls back to CPU on machines without
    an NVIDIA GPU. Any probing failure also falls back to CPU.
    """
    try:
        import paddle

        if paddle.device.is_compiled_with_cuda() and paddle.device.cuda.device_count() > 0:
            return "gpu"
    except Exception:
        logger.exception("Paddle GPU probe failed; using CPU for OCR")
    return "cpu"


@lru_cache(maxsize=1)
def _load_embedding_model() -> SentenceTransformer:
    return SentenceTransformer(
        EMBEDDING_MODEL,
        device=device(),
        model_kwargs={"torch_dtype": "auto"},
    )


def embedding_model() -> SentenceTransformer:
    with _embedding_model_lock:
        return _load_embedding_model()


@lru_cache(maxsize=1)
def _load_reranker_model() -> CrossEncoder:
    return CrossEncoder(
        RERANKER_MODEL,
        device=device(),
        max_length=8192,
        prompts={"screensearch": RETRIEVAL_INSTRUCTION},
        default_prompt_name="screensearch",
    )


def reranker_model() -> CrossEncoder:
    with _reranker_model_lock:
        return _load_reranker_model()


@lru_cache(maxsize=8)
def _load_ocr_model(language: str):
    from paddleocr import PaddleOCR

    return PaddleOCR(
        lang=language,
        ocr_version="PP-OCRv5",
        # Use the GPU when available; on the RTX-class CPU baseline a dense
        # screen takes ~60s/frame, while the same frame runs in ~1s on a CUDA
        # GPU. Falls back to CPU automatically (see paddle_device).
        device=paddle_device(),
        # MKLDNN/oneDNN crashes PP-OCRv5 detection under PaddleOCR 3.x's PIR
        # executor ("ConvertPirAttribute2RuntimeAttribute not support") and is a
        # CPU-only accelerator, so it stays disabled on both backends. OCR cost
        # is further reduced by downscaling frames before send (see
        # screensearch-capture/src/sidecar_ocr.rs) and off-loading inference to
        # the threadpool so it never blocks the event loop.
        enable_mkldnn=False,
        # Screen captures are always upright, so the document- and textline-
        # orientation classifiers are pure overhead (the textline classifier runs
        # once per detected line — dozens to hundreds per frame). Disable both.
        use_doc_orientation_classify=False,
        use_doc_unwarping=False,
        use_textline_orientation=False,
    )


def ocr_model(language: str):
    with _ocr_model_lock:
        return _load_ocr_model(language)


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
        "ocr_device": paddle_device(),
        "embedding_model": EMBEDDING_MODEL,
        "reranker_model": RERANKER_MODEL,
        "embedding_dimension": EMBEDDING_DIMENSION,
        "paddleocr_version": PADDLEOCR_VERSION,
    }


def _prepare_models(components: list[str], ocr_language: str) -> None:
    global _preparation_status

    ready: list[str] = []
    current_component: str | None = None
    try:
        for component in components:
            current_component = component
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
        # Use the locally tracked component instead of reading the shared
        # _preparation_status outside the lock.
        logger.exception(
            "Model preparation failed while initializing %s",
            current_component,
        )
        with _preparation_lock:
            _preparation_status = ModelPreparationStatus(
                state="error",
                current_component=current_component,
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
    raw = await image.read(MAX_OCR_UPLOAD_BYTES + 1)
    if len(raw) > MAX_OCR_UPLOAD_BYTES:
        raise HTTPException(
            status_code=413,
            detail=f"OCR image exceeds {MAX_OCR_UPLOAD_BYTES} bytes",
        )

    source = Image.open(BytesIO(raw))
    if source.width * source.height > MAX_OCR_PIXELS:
        raise HTTPException(
            status_code=413,
            detail=f"OCR image exceeds {MAX_OCR_PIXELS} pixels",
        )
    pixels = np.asarray(source.convert("RGB")).copy()
    # PaddleOCR's model load and predict are blocking and CPU-bound. Run them in
    # the threadpool so they never freeze the asyncio event loop; otherwise a
    # single slow OCR call would stall every other sidecar route (health,
    # /v1/models/status) for its entire duration.
    model = await run_in_threadpool(ocr_model, language)
    results = await run_in_threadpool(model.predict, pixels)
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

        texts = payload.get("rec_texts") or []
        scores = payload.get("rec_scores") or []
        boxes = payload.get("rec_boxes") or []
        if len(texts) != len(scores) or len(texts) != len(boxes):
            logger.warning(
                "Mismatched OCR result lengths: texts=%d, scores=%d, boxes=%d",
                len(texts),
                len(scores),
                len(boxes),
            )
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
