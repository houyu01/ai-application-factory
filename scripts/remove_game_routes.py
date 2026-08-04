"""Remove game endpoint definitions after they move into api.game_routes."""

from pathlib import Path

path = Path(__file__).resolve().parents[1] / "backend/src/api/router.py"
source = path.read_text()
marker = '@api_router.get("/games")'
if marker not in source:
    raise SystemExit("game route marker not found")
path.write_text(source.split(marker, 1)[0].rstrip() + "\n\nfrom . import game_routes  # noqa: F401  # register game endpoints on the shared router\n")
