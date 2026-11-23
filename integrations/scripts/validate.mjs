import assert from 'node:assert/strict';
import { readFileSync, realpathSync } from 'node:fs';
import { dirname, join, relative, resolve, sep } from 'node:path';
import { fileURLToPath } from 'node:url';
import Ajv from 'ajv';
import { parse } from 'yaml';
import { files, root, syncSkill } from './sync-skill.mjs';

export const json = (file) => JSON.parse(readFileSync(file, 'utf8'));
const read = (file) => readFileSync(file, 'utf8').replaceAll('\r\n', '\n');

export function checkSchema(name, value) {
  const validate = new Ajv({ allErrors: true }).compile(json(join(root, `schemas/${name}.json`)));
  assert(validate(value), `${name}: ${JSON.stringify(validate.errors)}`);
}

// Fail closed on an unfamiliar registration shape. Changes to tools::all() require
// updating this small source reader, never silently accepting a stale tool catalog.
// spec: AGENT-030
/** Tools a server offers by default: `tools::all()` without `AI_TOOLS`, which need the switch "KI (Vorschau)" (`tools::offered`). With `preview` all tools. */
export function registry(repo = resolve(root, '..'), { preview = false } = {}) {
  const dir = join(repo, 'crates/os-mcp/src/tools');
  const mod = read(join(dir, 'mod.rs'));
  const body = mod.split('pub fn all()')[1]?.split('#[cfg(test)]')[0];
  assert(body, 'tools::all() missing');
  const entries = [...body.matchAll(/Box::new\((\w+)::(\w+)(?:\("([^"]+)"\))?\)/g)];
  assert.equal(entries.length, [...body.matchAll(/Box::new/g)].length, 'Unrecognized registration');
  assert(entries.length > 0);
  const tools = {};
  for (const [, module, , literal] of entries) {
    const source = read(join(dir, `${module}.rs`));
    const name = literal ?? source.match(/pub const NAME: &str = "([^"]+)"/)?.[1]
      ?? source.match(/fn name\(&self\) -> &'static str\s*\{\s*"([^"]+)"\s*\}/)?.[1];
    const className = source.match(/fn class\(&self\) -> ToolClass\s*\{\s*ToolClass::(\w+)\s*\}/)?.[1]?.toLowerCase();
    assert(name && ['read', 'crawl', 'write', 'cost'].includes(className), `Unknown registration ${module}`);
    assert(!(name in tools), `Duplicate tool ${name}`);
    tools[name] = className;
  }
  if (preview) return tools;
  const ai = mod.match(/pub const AI_TOOLS: \[&str; (\d+)\] = \[([\s\S]*?)\];/);
  assert(ai, 'tools::AI_TOOLS missing');
  const aiNames = [...ai[2].matchAll(/"([^"]+)"/g)].map(([, name]) => name);
  assert.equal(aiNames.length, Number(ai[1]), 'Unrecognized AI_TOOLS entry');
  for (const name of aiNames) {
    assert(name in tools, `AI tool ${name} is not registered in tools::all()`);
    delete tools[name];
  }
  return tools;
}

export function frontmatter(text, skill = false) {
  const match = text.match(/^---\r?\n([\s\S]*?)\r?\n---\r?\n([\s\S]+)$/);
  assert(match, 'Missing frontmatter/body');
  const meta = parse(match[1], { uniqueKeys: true });
  assert(meta && typeof meta === 'object');
  assert(typeof meta.description === 'string' && meta.description.length > 0 && meta.description.length <= 1024);
  if (skill) {
    assert.match(meta.name, /^[a-z0-9]+(-[a-z0-9]+)*$/);
    assert(meta.name.length <= 64);
    assert.equal(meta.name, 'openspider');
    assert.deepEqual(Object.keys(meta).sort(), ['description', 'name']);
  }
  return meta;
}

export function checkReferences(dir) {
  const boundary = realpathSync(dir) + sep;
  for (const file of files(dir).filter((path) => path.endsWith('.md'))) {
    for (const [, target] of read(file).matchAll(/\[[^\]]+\]\(([^)#]+)(?:#[^)]*)?\)/g)) {
      if (/^https?:/.test(target)) continue;
      assert(realpathSync(resolve(dirname(file), target)).startsWith(boundary), `Reference escapes skill: ${target}`);
    }
  }
}

export function validate(base = root) {
  const marketplace = json(join(base, '.claude-plugin/marketplace.json'));
  checkSchema('marketplace', marketplace);
  assert.equal(marketplace.plugins.length, 1);
  const entry = marketplace.plugins[0];
  const pluginDir = resolve(base, entry.source);
  assert(relative(base, pluginDir) && !relative(base, pluginDir).startsWith('..'));
  const plugin = json(join(pluginDir, '.claude-plugin/plugin.json'));
  checkSchema('plugin', plugin);
  assert.equal(entry.name, plugin.name);
  checkSchema('mcp', json(join(pluginDir, '.mcp.json')));
  realpathSync(join(pluginDir, 'scripts/launch.mjs'));
  const skillDir = join(base, 'skills/openspider');
  frontmatter(read(join(skillDir, 'SKILL.md')), true);
  checkReferences(skillDir);
  syncSkill(true, base);
  const tools = registry();
  const fieldNames = new Set(['crawl_id', 'next_cursor', 'not_evaluated', 'filter_state', 'inlinks_cursor', 'outlinks_cursor']);
  const checkToolReferences = (file) => {
    for (const [, name] of read(file).matchAll(/`([a-z]+(?:_[a-z]+)+)`/g)) {
      assert(name in tools || fieldNames.has(name), `Unknown tool/field reference ${name}: ${file}`);
    }
  };
  const rows = [...read(join(skillDir, 'references/tools.md')).matchAll(/^\| `(\w+)` \| `(\w+)` \|/gm)];
  assert.deepEqual(Object.fromEntries(rows.map(([, name, cls]) => [name, cls])), tools, 'Tool catalog differs from the tools offered by default (tools::offered, switch off)');
  for (const file of files(skillDir).filter((path) => path.endsWith('.md'))) {
    if (relative(skillDir, file).split(sep)[0] !== 'evals') checkToolReferences(file);
    for (const [, block] of read(file).matchAll(/```json\n([\s\S]*?)\n```/g)) {
      for (const line of block.split('\n')) {
        const call = JSON.parse(line);
        if (call.tool) assert(call.tool in tools, `Unknown tool example ${call.tool}`);
      }
    }
  }
  const commands = files(join(pluginDir, 'commands'));
  assert.deepEqual(commands.map((file) => file.split(sep).at(-1)).sort(), ['openspider-audit.md', 'openspider-compare.md', 'openspider-issues.md']);
  for (const file of commands) {
    frontmatter(read(file));
    assert(read(file).includes('$ARGUMENTS'));
    checkToolReferences(file);
    for (const [, path] of read(file).matchAll(/\$\{CLAUDE_PLUGIN_ROOT\}\/([^`\s]+)/g)) {
      assert(realpathSync(join(pluginDir, path)).startsWith(realpathSync(pluginDir) + sep));
    }
  }
  const evals = json(join(skillDir, 'evals/tasks.json'));
  assert(evals.length >= 10);
  assert.equal(new Set(evals.map((task) => task.id)).size, evals.length);
  for (const task of evals) {
    assert(task.prompt && task.criteria.length > 0 && task.tools.length > 0);
    assert(['out-basic-site', 'out-redirects'].includes(task.fixture));
    realpathSync(join(root, '../tests/fixtures', task.fixture, 'scenario.toml'));
    for (const name of task.tools) assert(name in tools, `Eval references unknown tool ${name}`);
  }
  return { tools, tasks: evals.length };
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  console.log(JSON.stringify(validate(), null, 2));
}
