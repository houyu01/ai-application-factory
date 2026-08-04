"""Keep persistence tests in focused files without changing their assertions."""

from pathlib import Path

root = Path(__file__).resolve().parents[1]
source_path = root / "backend/tests/test_persistence.py"
lines = source_path.read_text().splitlines()
split_at = next(index for index, line in enumerate(lines) if line.startswith("def test_planner_ids_are_scoped_to_each_drama"))
header_end = next(index for index, line in enumerate(lines) if line.startswith("class FakePlanner:"))
header = lines[:header_end]
helper_end = next(index for index, line in enumerate(lines) if line.startswith("def test_create_persists_empty_drama"))
helpers = lines[header_end:helper_end]
new_content = header + helpers + [""] + lines[split_at:]
(source_path.parent / "test_persistence_tasks.py").write_text("\n".join(new_content).rstrip() + "\n")
source_path.write_text("\n".join(lines[:split_at]).rstrip() + "\n")
