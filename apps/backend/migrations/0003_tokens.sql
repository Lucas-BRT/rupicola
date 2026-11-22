-- One table for every credential: sessions, machine tokens and project devices.
-- Only SHA-256 digests of the secrets are stored. Existing panel credentials are
-- copied over so current sign-ins and tokens keep working.

CREATE TABLE tokens (
    digest TEXT PRIMARY KEY,
    kind TEXT NOT NULL,
    handle TEXT,
    project TEXT,
    label TEXT NOT NULL,
    created INTEGER NOT NULL,
    expires INTEGER,
    last_seen INTEGER
);

CREATE INDEX idx_tokens_handle ON tokens(handle);

INSERT INTO tokens (digest, kind, handle, project, label, created, expires, last_seen)
    SELECT id, 'session', handle, NULL, 'Web session', 0, expires, NULL FROM sessions;

INSERT INTO tokens (digest, kind, handle, project, label, created, expires, last_seen)
    SELECT digest, 'machine', owner, NULL, label, created, NULL, last_seen
    FROM machine_tokens;

INSERT INTO tokens (digest, kind, handle, project, label, created, expires, last_seen)
    SELECT digest, 'device', NULL, project, label, created, NULL, last_seen FROM devices;
