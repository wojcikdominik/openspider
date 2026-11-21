import { test } from 'node:test';
import assert from 'node:assert/strict';
import { mkdtempSync, readFileSync, rmSync, readdirSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { createHash } from 'node:crypto';
import { DatabaseSync } from 'node:sqlite';
import { archiveFixture, currentSchemaVersion, seedCopies } from '../../../../scripts/release/archive-compat-fixture.mjs';

// spec: OUT-255
test('release archive is complete, versioned, verifiable and immutable', () => {
  const out = mkdtempSync(join(tmpdir(), 'compat-'));
  try {
    const dir = archiveFixture({ version: '1.0.0-beta.1', out, commit: 'a'.repeat(40) });
    const manifest = JSON.parse(readFileSync(join(dir, 'manifest.json')));
    assert.equal(manifest.version, '1.0.0-beta.1');
    assert.equal(manifest.files.length, 8);
    for (const file of manifest.files) assert.equal(createHash('sha256').update(readFileSync(join(dir, file.name))).digest('hex'), file.sha256);
    const db = new DatabaseSync(join(dir, 'crawl.ospider'), { readOnly: true });
    try {
      assert.equal(db.prepare('PRAGMA user_version').get().user_version, currentSchemaVersion());
      assert.equal(manifest.schema, currentSchemaVersion());
      assert.equal(db.prepare("SELECT value FROM crawl_meta WHERE key='generator'").get().value, 'openspider 1.0.0-beta.1');
    } finally { db.close(); }
    assert.throws(() => archiveFixture({ version: '1.0.0-beta.1', out }), /already exists/);
    for (const version of ['../escape', '1.0.0-rc.0', 'v1.0.0', '1.0', '01.0.0', '1.00.0']) assert.throws(() => archiveFixture({ version, out }));
    assert.deepEqual(readdirSync(out), ['1.0.0-beta.1']);
    archiveFixture({ version: '1.0.0', out });
  } finally { rmSync(out, { recursive: true, force: true }); }
});

// spec: OUT-255
test('failed release export removes staging and never leaves a partial archive', () => {
  const out = mkdtempSync(join(tmpdir(), 'compat-failure-'));
  try {
    assert.throws(() => archiveFixture({ version: '1.0.0-beta.2', out, cli: join(out, 'missing-cli') }));
    assert.deepEqual(readdirSync(out), []);
  } finally { rmSync(out, { recursive: true, force: true }); }
});

// spec: OUT-255
test('R04 dry runs before beta.1 use the same versions as set-version', () => {
  const out = mkdtempSync(join(tmpdir(), 'compat-dry-run-'));
  try {
    for (const version of ['1.0.0-beta.0', '0.1.0']) {
      const dir = archiveFixture({ version, out });
      assert.equal(JSON.parse(readFileSync(join(dir, 'manifest.json'))).version, version);
    }
  } finally { rmSync(out, { recursive: true, force: true }); }
});

// spec: OUT-255
test('every archived schema seed matches its folder, including v5 as migration source', () => {
  assert.equal(currentSchemaVersion(), 6);
  for (const schema of [5, 6]) {
    const copies = seedCopies(schema);
    assert.equal(copies['crawl.ospider'], `schema-v${schema}/crawl.ospider`);
    const db = new DatabaseSync(join(import.meta.dirname, '..', copies['crawl.ospider']), { readOnly: true });
    try {
      assert.equal(db.prepare('PRAGMA user_version').get().user_version, schema);
    } finally { db.close(); }
  }
});
