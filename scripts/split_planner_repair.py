"""Move planner normalization and repair methods into a focused mixin."""

from __future__ import annotations

import ast
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
path = ROOT / "backend/src/llm_service/planner_decomposition_mixin.py"
source = path.read_text()
tree = ast.parse(source)
class_node = next(node for node in tree.body if isinstance(node, ast.ClassDef))
selected = {"_repair_asset_catalog", "_normalize_plan"}
methods = [node for node in class_node.body if isinstance(node, ast.FunctionDef) and node.name in selected]
lines = source.splitlines()
header = "\n".join(lines[: class_node.lineno - 1]).rstrip()
body = [header, "", "class ScriptPlannerRepairMixin:", '    """Normalize provider output and repair incomplete drama plans."""', ""]
for node in methods:
    body.extend(lines[node.lineno - 1 : node.end_lineno])
    body.append("")
(path.parent / "planner_repair_mixin.py").write_text("\n".join(body).rstrip() + "\n")

for node in sorted(methods, key=lambda item: item.lineno, reverse=True):
    del lines[node.lineno - 1 : node.end_lineno]
path.write_text("\n".join(lines).rstrip() + "\n")
