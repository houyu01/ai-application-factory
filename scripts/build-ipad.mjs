import { existsSync, readFileSync } from 'node:fs';
import { spawnSync } from 'node:child_process';
import { resolve } from 'node:path';
import process from 'node:process';

const signingFile = resolve('.env.ios');

function loadSigningEnvironment(file) {
  for (const [lineNumber, rawLine] of readFileSync(file, 'utf8').split(/\r?\n/).entries()) {
    const line = rawLine.trim();
    if (!line || line.startsWith('#')) continue;
    const match = line.match(/^([A-Z][A-Z0-9_]*)=(.*)$/);
    if (!match) {
      throw new Error(`${file}:${lineNumber + 1} 必须使用 KEY=VALUE 格式。`);
    }
    const [, key, rawValue] = match;
    const value = rawValue.trim().replace(/^(['"])(.*)\1$/, '$2');
    if (process.env[key] === undefined) process.env[key] = value;
  }
}

if (!existsSync(signingFile)) {
  console.error('缺少 .env.ios。请先执行 cp .env.ios.example .env.ios 并填写 APPLE_DEVELOPMENT_TEAM。');
  process.exit(1);
}

try {
  loadSigningEnvironment(signingFile);
} catch (error) {
  console.error(error instanceof Error ? error.message : String(error));
  process.exit(1);
}

const teamId = process.env.APPLE_DEVELOPMENT_TEAM;
if (!teamId || teamId === 'YOUR_10_CHARACTER_TEAM_ID') {
  console.error('.env.ios 中的 APPLE_DEVELOPMENT_TEAM 尚未填写。');
  process.exit(1);
}

const result = spawnSync(
  'npx',
  ['tauri', 'ios', 'build', '--ci', '--target', 'aarch64', '--export-method', 'app-store-connect'],
  { env: process.env, stdio: 'inherit' },
);

if (result.error) throw result.error;
process.exit(result.status ?? 1);
