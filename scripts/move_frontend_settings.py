"""Move settings helper functions into frontend/src/settings_ui.ts."""

from pathlib import Path

path = Path(__file__).resolve().parents[1] / "frontend/src/main.ts"
lines = path.read_text().splitlines()
start = next(index for index, line in enumerate(lines) if line.startswith("function modelChoices"))
end = next(index for index, line in enumerate(lines[start:], start) if line.startswith("function openConfiguredDramaModal"))
del lines[start:end]
path.write_text("\n".join(lines).rstrip() + "\n")
