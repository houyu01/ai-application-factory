"""FastAPI application entry point."""
from contextlib import asynccontextmanager

from fastapi import FastAPI
from fastapi.middleware.cors import CORSMiddleware
from .api.router import api_router
from .application.task_worker import durable_task_worker

@asynccontextmanager
async def lifespan(_app: FastAPI):
    durable_task_worker.start()
    try:
        yield
    finally:
        durable_task_worker.stop()


app = FastAPI(
    title="AI Application Factory API",
    version="0.1.0",
    lifespan=lifespan,
)
app.add_middleware(
    CORSMiddleware,
    allow_origins=[
        "http://127.0.0.1:5173",
        "http://localhost:5173",
    ],
    allow_origin_regex=r"https?://(localhost|127\.0\.0\.1):\d+",
    allow_credentials=True,
    allow_methods=["*"],
    allow_headers=["*"],
)
app.include_router(api_router, prefix="/api")


@app.get("/health")
def health_check() -> dict[str, str]:
    """Return liveness for the frontend or process supervisor health probe."""
    return {"status": "ok"}
