const fs = require('node:fs');
const path = require('node:path');
const kdbxweb = require(process.argv[2] || 'kdbxweb');

async function generate(major, filename) {
    const password = kdbxweb.ProtectedValue.fromString('correct horse battery staple');
    const db = kdbxweb.Kdbx.create(new kdbxweb.Credentials(password), 'Independent upgrade fixture');
    db.setVersion(major);
    db.header.versionMinor = major === 3 ? 1 : 0;
    if (major === 3) {
        db.header.keyEncryptionRounds = 10;
    } else {
        db.setKdf(kdbxweb.Consts.KdfId.Aes);
        db.header.kdfParameters.set('R', kdbxweb.VarDictionary.ValueType.UInt64, kdbxweb.Int64.from(10));
    }
    db.meta.desc = 'Fake credentials only. Low-cost KDF for regression tests.';
    db.meta.customData = new Map([['fixture.meta', { value: 'metadata survives' }]]);
    const group = db.createGroup(db.getDefaultGroup(), 'api keys');
    group.notes = 'Group metadata survives';
    const entry = db.createEntry(group);
    entry.fields.set('Title', 'Foreign fixture');
    entry.fields.set('UserName', 'foreign-user');
    entry.fields.set('Password', kdbxweb.ProtectedValue.fromString('old-fake-secret'));
    entry.fields.set('Plugin-protected', kdbxweb.ProtectedValue.fromString('protected custom value'));
    const binary = await db.createBinary(kdbxweb.ProtectedValue.fromString('attachment fixture bytes'));
    entry.binaries.set('fixture.txt', binary);
    entry.pushHistory();
    entry.fields.set('Password', kdbxweb.ProtectedValue.fromString('current-fake-secret'));
    entry.customData = new Map([['fixture.entry', { value: 'entry custom data survives' }]]);
    const unrelated = db.createEntry(db.createGroup(db.getDefaultGroup(), 'Unrelated'));
    unrelated.fields.set('Title', 'Other entry');
    unrelated.fields.set('Password', kdbxweb.ProtectedValue.fromString('unrelated-fake-secret'));
    unrelated.binaries.set('shared.txt', binary);
    const historical = db.createEntry(unrelated.parentGroup);
    historical.fields.set('Title', 'Historical attachment owner');
    historical.fields.set('Password', kdbxweb.ProtectedValue.fromString('historical-fake-secret'));
    historical.binaries.set('history-only.txt', binary);
    historical.pushHistory();
    historical.binaries.clear();
    fs.writeFileSync(path.join(__dirname, filename), Buffer.from(await db.save()));
}

Promise.all([generate(3, 'kdbx3.kdbx'), generate(4, 'kdbx40.kdbx')]).catch(error => {
    console.error(error);
    process.exitCode = 1;
});
