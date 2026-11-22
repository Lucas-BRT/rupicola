-- Tables the web panel created on its own before the backend owned the database.
-- IF NOT EXISTS keeps an existing panel.db intact.

CREATE TABLE IF NOT EXISTS accounts (
    handle TEXT PRIMARY KEY, password TEXT NOT NULL, owner INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS sessions (
    id TEXT PRIMARY KEY, handle TEXT NOT NULL, csrf TEXT NOT NULL, expires INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS projects (id TEXT PRIMARY KEY, label TEXT NOT NULL);

CREATE TABLE IF NOT EXISTS members (
    project TEXT NOT NULL, handle TEXT NOT NULL, role TEXT NOT NULL,
    PRIMARY KEY(project, handle)
);

CREATE TABLE IF NOT EXISTS invites (
    digest TEXT PRIMARY KEY, project TEXT NOT NULL, role TEXT NOT NULL,
    expires INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS devices (
    digest TEXT PRIMARY KEY, project TEXT NOT NULL, label TEXT NOT NULL,
    created INTEGER NOT NULL, last_seen INTEGER
);

CREATE TABLE IF NOT EXISTS machine_tokens (
    digest TEXT PRIMARY KEY, owner TEXT NOT NULL, label TEXT NOT NULL,
    created INTEGER NOT NULL, last_seen INTEGER
);

CREATE TABLE IF NOT EXISTS audit (
    id INTEGER PRIMARY KEY, project TEXT NOT NULL, actor TEXT NOT NULL,
    event TEXT NOT NULL, occurred INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS design_versions_v1 (
    project TEXT NOT NULL, id TEXT NOT NULL, generation INTEGER NOT NULL,
    created INTEGER NOT NULL, name TEXT NOT NULL, phase TEXT NOT NULL,
    payload TEXT NOT NULL, PRIMARY KEY(project, id)
);

CREATE TABLE IF NOT EXISTS design_adoption_v1 (
    project TEXT PRIMARY KEY, version TEXT NOT NULL
);
