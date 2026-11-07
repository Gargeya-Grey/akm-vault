// Second-reader interchange test: AKM vaults read and written by kdbxweb, using fake keys only.
import { spawnSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import path from 'node:path';
import { argon2d, argon2id } from 'hash-wasm';
import kdbxweb from 'kdbxweb';

const { Kdbx, KdbxCredentials, ProtectedValue, Consts, CryptoEngine, Int64, VarDictionary } = kdbxweb;

const PASSWORD = 'interop test correct horse battery staple';
const ARGON2_MEMORY_BYTES = 64 * 1024 * 1024;

function parseArgs(argv) {
  const index = argv.indexOf('--akm');
  if (index < 0 || !argv[index + 1]) {
    console.error('Usage: node verify.mjs --akm <path to akm.exe>');
    process.exit(2);
  }
  return { akm: path.resolve(argv[index + 1]) };
}

CryptoEngine.setArgon2Impl(async (password, salt, memoryKiB, iterations, length, parallelism, type, version) => {
  if (version !== 0x13) throw new Error(`Argon2 version ${version} is not supported by hash-wasm`);
  const run = type === CryptoEngine.Argon2TypeArgon2id ? argon2id : argon2d;
  const out = await run({
    password: new Uint8Array(password),
    salt: new Uint8Array(salt),
    parallelism,
    iterations,
    memorySize: memoryKiB,
    hashLength: length,
    outputType: 'binary',
  });
  return out.buffer.slice(out.byteOffset, out.byteOffset + out.byteLength);
});

const toArrayBuffer = (buffer) => buffer.buffer.slice(buffer.byteOffset, buffer.byteOffset + buffer.byteLength);
const sha256 = (bytes) => createHash('sha256').update(Buffer.from(bytes)).digest('hex');
const credentials = () => new KdbxCredentials(ProtectedValue.fromString(PASSWORD));

function uuidText(uuid) {
  const hex = Buffer.from(uuid.id, 'base64').toString('hex');
  return `${hex.slice(0, 8)}-${hex.slice(8, 12)}-${hex.slice(12, 16)}-${hex.slice(16, 20)}-${hex.slice(20)}`;
}

function equal(actual, expected, label) {
  const a = JSON.stringify(actual);
  const e = JSON.stringify(expected);
  if (a !== e) throw new Error(`${label}: expected ${e}, got ${a}`);
}

function differences(actual, expected, where = '$') {
  if (JSON.stringify(actual) === JSON.stringify(expected)) return [];
  const isObject = (value) => value !== null && typeof value === 'object';
  if (!isObject(actual) || !isObject(expected) || Array.isArray(actual) !== Array.isArray(expected)) {
    const short = (value) => {
      const text = JSON.stringify(value);
      return text !== undefined && text.length > 120 ? `${text.slice(0, 120)}...` : text;
    };
    return [`${where}: expected ${short(expected)}, got ${short(actual)}`];
  }
  const keys = new Set([...Object.keys(actual), ...Object.keys(expected)]);
  return [...keys].flatMap((key) => differences(actual[key], expected[key], `${where}.${key}`));
}

function same(actual, expected, label) {
  const found = differences(actual, expected);
  if (found.length > 0) throw new Error(`${label} differs at ${found.slice(0, 6).join('; ')}`);
}

function isTrue(condition, label) {
  if (!condition) throw new Error(label);
}

const failures = [];
async function check(name, body) {
  try {
    await body();
    console.log(`PASS: ${name}`);
  } catch (error) {
    failures.push(name);
    console.log(`FAIL: ${name}: ${error.message}`);
  }
}

function createRunner(akmExe, vault) {
  const env = {};
  for (const [name, value] of Object.entries(process.env)) {
    if (!name.toUpperCase().startsWith('AKM_')) env[name] = value;
  }
  env.AKM_PASSWORD = PASSWORD;
  return (args, input) => {
    const result = spawnSync(akmExe, ['--vault', vault, ...args], {
      env,
      input,
      encoding: 'utf8',
      timeout: 120000,
    });
    if (result.error) throw result.error;
    return { code: result.status, out: result.stdout, err: result.stderr };
  };
}

async function loadVault(file) {
  return Kdbx.load(toArrayBuffer(readFileSync(file)), credentials());
}

function fieldText(entry, name) {
  const value = entry.fields.get(name);
  return value instanceof ProtectedValue ? value.getText() : value;
}

function allEntries(db) {
  return [...db.getDefaultGroup().allEntries()];
}

function findEntry(db, title) {
  const matches = allEntries(db).filter((entry) => fieldText(entry, 'Title') === title);
  if (matches.length !== 1) throw new Error(`expected one entry titled ${JSON.stringify(title)}, found ${matches.length}`);
  return matches[0];
}

function kdfSnapshot(db) {
  const params = db.header.kdfParameters;
  return {
    id: Buffer.from(params.get('$UUID')).toString('base64'),
    memory: Number(params.get('M')),
    iterations: Number(params.get('I')),
    parallelism: Number(params.get('P')),
    version: Number(params.get('V')),
  };
}

const EXPECTED_KDF = {
  id: Consts.KdfId.Argon2id,
  memory: ARGON2_MEMORY_BYTES,
  iterations: 3,
  parallelism: 4,
  version: 0x13,
};

function binaryBytes(binary) {
  const value = binary.value ?? binary;
  return value instanceof ProtectedValue ? value.getBinary() : value;
}

function fieldSnapshot(entry) {
  const fields = {};
  for (const [name, value] of [...entry.fields.entries()].sort(([a], [b]) => a.localeCompare(b))) {
    fields[name] = value instanceof ProtectedValue ? { protected: true, text: value.getText() } : { protected: false, text: value };
  }
  return fields;
}

function entrySnapshot(entry, withHistory = true) {
  const binaries = {};
  for (const [name, binary] of [...entry.binaries.entries()].sort(([a], [b]) => a.localeCompare(b))) {
    const bytes = binaryBytes(binary);
    binaries[name] = { length: bytes.byteLength, sha256: sha256(bytes), hex: Buffer.from(bytes).toString('hex') };
  }
  return {
    uuid: entry.uuid.id,
    fields: fieldSnapshot(entry),
    tags: [...entry.tags],
    created: entry.times.creationTime?.getTime() ?? null,
    modified: entry.times.lastModTime?.getTime() ?? null,
    binaries,
    history: withHistory ? entry.history.map((item) => entrySnapshot(item, false)) : undefined,
  };
}

function groupSnapshot(group) {
  return {
    uuid: group.uuid.id,
    name: group.name,
    // KeePass readers treat an empty <Notes/> and a missing element as the same value.
    notes: group.notes ?? '',
    created: group.times.creationTime?.getTime() ?? null,
    entries: group.entries.map((entry) => entrySnapshot(entry)).sort((a, b) => a.uuid.localeCompare(b.uuid)),
    groups: group.groups.map(groupSnapshot).sort((a, b) => a.uuid.localeCompare(b.uuid)),
  };
}

function findGroup(db, name) {
  const matches = [...db.getDefaultGroup().allGroups()].filter((group) => group.name === name);
  if (matches.length !== 1) throw new Error(`expected one group named ${JSON.stringify(name)}, found ${matches.length}`);
  return matches[0];
}

async function partA(akmExe, work) {
  console.log('--- A: vault created by AKM, read by kdbxweb ---');
  const vault = path.join(work, 'created-by-akm.kdbx');
  const akm = createRunner(akmExe, vault);
  const state = {};

  await check('A1 AKM creates a vault and adds fake keys', () => {
    const steps = [
      [['init', '--name', 'kdbxweb second reader']],
      [
        ['add', 'OpenAI interop', '--project', 'demo', '--env', 'dev', '--issuer', 'openai', '--scopes', 'read,write',
          '--env-var', 'OPENAI_API_KEY', '--url', 'https://example.invalid/openai', '--notes', 'Fake note é ✓',
          '--tags', 'fake,ai', '--expires', '2027-03-15', '--rotation-days', '90', '--username', 'svc-user', '--secret-stdin'],
        'fake-openai-secret\n',
      ],
      [['add', 'Plain minimal', '--secret-stdin'], 'fake-minimal-secret\n'],
      [
        ['add', 'Stripe prod', '--project', 'demo', '--env', 'prod', '--issuer', 'stripe', '--secret-stdin'],
        'fake-stripe-secret\n',
      ],
    ];
    for (const [args, input] of steps) {
      const result = akm(args, input);
      if (result.code !== 0) throw new Error(`akm ${args[0]} exited ${result.code}: ${result.err}`);
    }
    state.listed = JSON.parse(akm(['list', '--json']).out);
    equal(state.listed.length, 3, 'akm list count');
  });

  let db = null;
  await check('A2 kdbxweb opens the vault with the master password', async () => {
    db = await loadVault(vault);
  });
  await check('A3 wrong master password is rejected by kdbxweb', async () => {
    let rejected = false;
    try {
      await Kdbx.load(toArrayBuffer(readFileSync(vault)), new KdbxCredentials(ProtectedValue.fromString('not the right password')));
    } catch (error) {
      rejected = error.code === Consts.ErrorCodes.InvalidKey;
    }
    isTrue(rejected, 'kdbxweb accepted a wrong password or failed with another error');
  });

  const needDb = () => {
    if (!db) throw new Error('precondition failed: A2 did not open the vault');
    return db;
  };

  await check('A4 file version is KDBX 4.1', () => {
    const d = needDb();
    equal([d.versionMajor, d.versionMinor], [4, 1], 'version');
  });
  await check('A5 KDF is Argon2id, 64 MiB, 3 iterations, parallelism 4', () => {
    equal(kdfSnapshot(needDb()), EXPECTED_KDF, 'kdf');
  });
  await check('A6 outer cipher is AES-256', () => {
    equal(needDb().header.dataCipherUuid.id, Consts.CipherId.Aes, 'cipher id');
  });
  await check('A7 inner stream is ChaCha20 and compression is GZip', () => {
    const d = needDb();
    equal(d.header.crsAlgorithm, Consts.CrsAlgorithm.ChaCha20, 'inner stream');
    equal(d.header.compression, Consts.CompressionAlgorithm.GZip, 'compression');
  });
  await check('A8 database name and the "API Keys" group', () => {
    const d = needDb();
    equal(d.meta.name, 'kdbxweb second reader', 'database name');
    const group = findGroup(d, 'API Keys');
    equal(group.entries.length, 3, 'entries in API Keys');
  });
  await check('A9 full entry: standard fields, tags, expiry, protected password', () => {
    const entry = findEntry(needDb(), 'OpenAI interop');
    equal(fieldText(entry, 'UserName'), 'svc-user', 'UserName');
    equal(fieldText(entry, 'URL'), 'https://example.invalid/openai', 'URL');
    equal(fieldText(entry, 'Notes'), 'Fake note é ✓', 'Notes');
    isTrue(entry.fields.get('Password') instanceof ProtectedValue, 'Password is not a protected value');
    equal(fieldText(entry, 'Password'), 'fake-openai-secret', 'Password');
    equal([...entry.tags].sort(), ['ai', 'fake'], 'tags');
    equal(entry.times.expires, true, 'expires flag');
    equal(entry.times.expiryTime.toISOString(), '2027-03-15T00:00:00.000Z', 'expiry time');
    isTrue(entry.times.creationTime instanceof Date, 'creation time missing');
  });
  await check('A10 full entry: every AKM custom field', () => {
    const entry = findEntry(needDb(), 'OpenAI interop');
    const listed = state.listed.find((key) => key.title === 'OpenAI interop');
    equal(fieldText(entry, 'AKM_ID'), listed.id, 'AKM_ID equals the id AKM lists');
    equal(uuidText(entry.uuid), listed.id, 'entry UUID equals the id AKM lists');
    equal(fieldText(entry, 'AKM_ISSUER'), 'openai', 'AKM_ISSUER');
    equal(fieldText(entry, 'AKM_PROJECT'), 'demo', 'AKM_PROJECT');
    equal(fieldText(entry, 'AKM_ENV'), 'dev', 'AKM_ENV');
    equal(JSON.parse(fieldText(entry, 'AKM_SCOPES')), ['read', 'write'], 'AKM_SCOPES');
    equal(fieldText(entry, 'AKM_ENV_VAR'), 'OPENAI_API_KEY', 'AKM_ENV_VAR');
    equal(fieldText(entry, 'AKM_ROTATION_DAYS'), '90', 'AKM_ROTATION_DAYS');
    for (const name of ['AKM_ID', 'AKM_ISSUER', 'AKM_PROJECT', 'AKM_ENV', 'AKM_SCOPES', 'AKM_ENV_VAR', 'AKM_ROTATION_DAYS']) {
      isTrue(typeof entry.fields.get(name) === 'string', `${name} is stored protected; expected plain text`);
    }
  });
  await check('A11 minimal entry: default username, empty scopes, no optional AKM fields', () => {
    const entry = findEntry(needDb(), 'Plain minimal');
    equal(fieldText(entry, 'UserName'), 'api-key', 'UserName');
    equal(fieldText(entry, 'Password'), 'fake-minimal-secret', 'Password');
    equal(JSON.parse(fieldText(entry, 'AKM_SCOPES')), [], 'AKM_SCOPES');
    equal(entry.times.expires, false, 'expires flag');
    for (const name of ['AKM_ISSUER', 'AKM_PROJECT', 'AKM_ENV', 'AKM_ENV_VAR', 'AKM_ROTATION_DAYS']) {
      isTrue(!entry.fields.has(name), `${name} should be absent`);
    }
    isTrue(typeof fieldText(entry, 'AKM_ID') === 'string' && fieldText(entry, 'AKM_ID').length === 36, 'AKM_ID missing');
  });
  await check('A12 third entry: project and environment', () => {
    const entry = findEntry(needDb(), 'Stripe prod');
    equal(fieldText(entry, 'AKM_PROJECT'), 'demo', 'AKM_PROJECT');
    equal(fieldText(entry, 'AKM_ENV'), 'prod', 'AKM_ENV');
    equal(fieldText(entry, 'AKM_ISSUER'), 'stripe', 'AKM_ISSUER');
    equal(fieldText(entry, 'Password'), 'fake-stripe-secret', 'Password');
  });
  await check('A13 kdbxweb saves the AKM vault and AKM still reads it', async () => {
    const d = needDb();
    const copy = path.join(work, 'resaved-by-kdbxweb.kdbx');
    writeFileSync(copy, Buffer.from(await d.save()));
    const reread = createRunner(akmExe, copy)(['list', '--json']);
    equal(reread.code, 0, `akm list exit code (${reread.err})`);
    equal(JSON.parse(reread.out).map((key) => key.title).sort(), ['OpenAI interop', 'Plain minimal', 'Stripe prod'], 'titles');
  });
}

function binaryFixture() {
  const bytes = Buffer.alloc(1024);
  for (let i = 0; i < bytes.length; i += 1) bytes[i] = (i * 7 + 13) % 256;
  return bytes;
}

async function buildForeignVault(file) {
  const db = Kdbx.create(credentials(), 'kdbxweb-written vault');
  db.setVersion(4);
  db.setKdf(Consts.KdfId.Argon2id);
  const params = db.header.kdfParameters;
  params.set('M', VarDictionary.ValueType.UInt64, Int64.from(ARGON2_MEMORY_BYTES));
  params.set('I', VarDictionary.ValueType.UInt64, Int64.from(3));
  params.set('P', VarDictionary.ValueType.UInt32, 4);

  const root = db.getDefaultGroup();
  const akmGroup = db.createGroup(root, 'API Keys');

  const akmEntry = db.createEntry(akmGroup);
  akmEntry.fields.set('Title', 'Kdbxweb AKM key');
  akmEntry.fields.set('UserName', 'api-key');
  akmEntry.fields.set('Password', ProtectedValue.fromString('fake-kdbxweb-secret'));
  akmEntry.fields.set('URL', 'https://example.invalid/kdbxweb');
  akmEntry.fields.set('Notes', 'Written by kdbxweb');
  akmEntry.fields.set('AKM_ID', uuidText(akmEntry.uuid));
  akmEntry.fields.set('AKM_PROJECT', 'demo');
  akmEntry.fields.set('AKM_ENV', 'dev');
  akmEntry.fields.set('AKM_SCOPES', '["read"]');
  akmEntry.fields.set('AKM_ENV_VAR', 'KDBXWEB_API_KEY');
  akmEntry.tags = ['kdbxweb'];

  const foreign = db.createEntry(akmGroup);
  foreign.fields.set('Title', 'Foreign entry');
  foreign.fields.set('UserName', 'foreign-user');
  foreign.fields.set('Password', ProtectedValue.fromString('fake-foreign-v1'));
  foreign.fields.set('Notes', 'revision 1');
  foreign.fields.set('Custom foreign field', 'plain custom value');
  foreign.fields.set('Custom protected field', ProtectedValue.fromString('fake-protected-custom'));
  foreign.tags = ['foreign', 'history'];
  foreign.pushHistory();
  foreign.fields.set('Password', ProtectedValue.fromString('fake-foreign-v2'));
  foreign.fields.set('Notes', 'revision 2');
  foreign.pushHistory();
  foreign.fields.set('Password', ProtectedValue.fromString('fake-foreign-v3'));
  foreign.fields.set('Notes', 'revision 3');
  const attachment = await db.createBinary(toArrayBuffer(binaryFixture()));
  foreign.binaries.set('foreign-attachment.bin', attachment);
  foreign.pushHistory();
  foreign.fields.set('Notes', 'revision 4 (current)');

  const unrelated = db.createGroup(root, 'Unrelated');
  unrelated.notes = 'Foreign group note';
  const other = db.createEntry(unrelated);
  other.fields.set('Title', 'Foreign other');
  other.fields.set('UserName', 'other-user');
  other.fields.set('Password', ProtectedValue.fromString('fake-unrelated-secret'));
  const nested = db.createGroup(unrelated, 'Nested');
  const deep = db.createEntry(nested);
  deep.fields.set('Title', 'Foreign nested');
  deep.fields.set('Password', ProtectedValue.fromString('fake-nested-secret'));

  writeFileSync(file, Buffer.from(await db.save()));
  return { akmEntryId: uuidText(akmEntry.uuid), foreignUuid: foreign.uuid.id };
}

function foreignSnapshot(db, foreignUuid) {
  const foreign = allEntries(db).find((entry) => entry.uuid.id === foreignUuid);
  if (!foreign) throw new Error('foreign entry is missing');
  return {
    entry: entrySnapshot(foreign),
    group: groupSnapshot(findGroup(db, 'Unrelated')),
  };
}

async function partB(akmExe, work) {
  console.log('--- B: vault written by kdbxweb, read and changed by AKM ---');
  const vault = path.join(work, 'written-by-kdbxweb.kdbx');
  const akm = createRunner(akmExe, vault);
  const state = {};

  await check('B1 kdbxweb writes an AKM-shaped entry, a foreign group, and a foreign entry with history and an attachment', async () => {
    Object.assign(state, await buildForeignVault(vault));
    const db = await loadVault(vault);
    equal([db.versionMajor, db.versionMinor], [4, 0], 'fixture version');
    equal(kdfSnapshot(db), EXPECTED_KDF, 'fixture kdf');
    const foreign = findEntry(db, 'Foreign entry');
    equal(foreign.history.length, 3, 'fixture history length');
    equal(foreign.binaries.size, 1, 'fixture attachment count');
    state.baseline = foreignSnapshot(db, state.foreignUuid);
  });

  await check('B2 AKM reads the kdbxweb-written AKM entry (metadata and secret)', () => {
    if (!state.akmEntryId) throw new Error('precondition failed: B1 did not run');
    const listed = akm(['list', '--json']);
    equal(listed.code, 0, `akm list exit code (${listed.err})`);
    const key = JSON.parse(listed.out).find((item) => item.title === 'Kdbxweb AKM key');
    isTrue(key, 'AKM does not list the kdbxweb-written entry');
    equal(key.id, state.akmEntryId, 'id');
    equal(key.project, 'demo', 'project');
    equal(key.environment, 'dev', 'environment');
    equal(key.scopes, ['read'], 'scopes');
    equal(key.env_var, 'KDBXWEB_API_KEY', 'env_var');
    const secret = akm(['get', state.akmEntryId, '--field', 'secret']);
    equal(secret.code, 0, `akm get exit code (${secret.err})`);
    equal(secret.out.trim(), 'fake-kdbxweb-secret', 'secret');
  });

  await check('B3 AKM refuses to write a KDBX 4.0 vault and leaves the file unchanged', () => {
    const before = sha256(readFileSync(vault));
    const result = akm(['add', 'Must not be saved', '--secret-stdin'], 'fake\n');
    isTrue(result.code !== 0, 'akm add succeeded on a KDBX 4.0 vault');
    equal(sha256(readFileSync(vault)), before, 'vault hash after refused write');
  });

  await check('B4 explicit upgrade makes the vault KDBX 4.1 and keeps every foreign byte', async () => {
    const result = akm(['upgrade', '--yes']);
    equal(result.code, 0, `akm upgrade exit code (${result.err})`);
    const db = await loadVault(vault);
    equal([db.versionMajor, db.versionMinor], [4, 1], 'version after upgrade');
    same(foreignSnapshot(db, state.foreignUuid), state.baseline, 'foreign data after upgrade');
  });

  await check('B5 AKM changes keys (adds one, removes the kdbxweb-written one) and the foreign data survives byte for byte', async () => {
    const added = akm(['add', 'Post interop key', '--project', 'demo', '--env', 'dev', '--secret-stdin'], 'fake-post-secret\n');
    equal(added.code, 0, `akm add exit code (${added.err})`);
    const removed = akm(['rm', state.akmEntryId, '--yes']);
    equal(removed.code, 0, `akm rm exit code (${removed.err})`);
    const db = await loadVault(vault);
    same(foreignSnapshot(db, state.foreignUuid), state.baseline, 'foreign data after AKM changes');
    equal(findEntry(db, 'Post interop key').fields.get('Password').getText(), 'fake-post-secret', 'new secret');
    isTrue(!allEntries(db).some((entry) => fieldText(entry, 'Title') === 'Kdbxweb AKM key'), 'removed entry is still present');
  });

  await check('B6 kdbxweb still reads the Argon2id 64 MiB / 3 / 4 KDF and AES-256 after AKM rewrites the vault', async () => {
    const db = await loadVault(vault);
    equal(kdfSnapshot(db), EXPECTED_KDF, 'kdf');
    equal(db.header.dataCipherUuid.id, Consts.CipherId.Aes, 'cipher id');
  });
}

async function main() {
  const { akm } = parseArgs(process.argv.slice(2));
  const work = mkdtempSync(path.join(tmpdir(), 'akm-kdbxweb-'));
  try {
    await partA(akm, work);
    await partB(akm, work);
  } finally {
    rmSync(work, { recursive: true, force: true });
  }
  if (failures.length > 0) {
    console.log(`FAIL: ${failures.length} check(s) failed`);
    process.exit(1);
  }
  const hash = sha256(readFileSync(akm));
  console.log(`PASS: all kdbxweb interchange checks. AKM SHA256=${hash}`);
}

main().catch((error) => {
  console.log(`FAIL: unexpected error: ${error.stack ?? error}`);
  process.exit(1);
});
