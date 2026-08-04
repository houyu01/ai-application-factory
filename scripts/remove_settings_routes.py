"""Move settings and template endpoints out of the main FastAPI router module."""

from pathlib import Path

path = Path(__file__).resolve().parents[1] / "backend/src/api/router.py"
source = path.read_text()
source = source.replace("from pydantic import BaseModel, Field\n", "")
start = source.index("class ModelConfig(BaseModel):")
end = source.index('@api_router.get("/media/{media_id}")')
source = source[:start] + source[end:]
start = source.index('@api_router.get("/prompt-templates")')
end = source.index('from . import game_routes')
source = source[:start] + "from . import settings_routes  # noqa: F401  # register settings endpoints\n\n" + source[end:]
path.write_text(source.rstrip() + "\n")
