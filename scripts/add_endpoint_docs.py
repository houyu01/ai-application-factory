"""Add concise frontend-trigger documentation to FastAPI endpoint functions."""

from __future__ import annotations

import ast
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
for relative in ("backend/src/api/router.py", "backend/src/api/game_routes.py"):
    path = ROOT / relative
    source = path.read_text()
    tree = ast.parse(source)
    lines = source.splitlines()
    insertions: list[tuple[int, str]] = []
    for node in tree.body:
        if not isinstance(node, ast.FunctionDef):
            continue
        if not any("api_router" in ast.unparse(decorator) for decorator in node.decorator_list):
            continue
        first = node.body[0]
        if isinstance(first, ast.Expr) and isinstance(first.value, ast.Constant) and isinstance(first.value.value, str):
            continue
        text = (
            f'    """Frontend route: called when the console performs the {node.name.replace("_", " ")} action; '
            'returns the persisted result or an asynchronous task status."""'
        )
        insertions.append((first.lineno - 1, text))
    for line_number, text in reversed(insertions):
        lines.insert(line_number, text)
    path.write_text("\n".join(lines).rstrip() + "\n")
