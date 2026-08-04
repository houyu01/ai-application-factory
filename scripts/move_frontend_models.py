"""Move shared frontend API types to a dedicated module."""

from pathlib import Path

path = Path(__file__).resolve().parents[1] / "frontend/src/main.ts"
lines = path.read_text().splitlines()
start = next(index for index, line in enumerate(lines) if line.startswith("type Project ="))
end = next(index for index, line in enumerate(lines[start:], start) if line.startswith("const API_BASE_URL"))
lines[start:end] = ["import type { ApiGame, ApiProject, DramaAsset, DramaAssetImageHistory, DramaAssetKind, DramaAssetMetadata, DramaAssetVariant, DramaEpisode, DramaPlacement, DramaPromptAssetType, DramaPromptNode, DramaShot, DramaShotVersion, Game, GameAsset, GameEdge, GameNode, GameTask, GenerationTask, Locale, ModelKind, ModelSettings, Project, VoicePreset } from './models';"]
path.write_text("\n".join(lines).rstrip() + "\n")
