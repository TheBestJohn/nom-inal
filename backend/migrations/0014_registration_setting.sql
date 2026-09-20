-- Whether new accounts may be created, as an instance setting.
--
-- "Close sign-ups once my household has joined" is the most common thing a
-- self-hosted instance needs after its first week, and it was an environment
-- variable: shell access, an edit to .env, and a container restart to change
-- it. It is policy about who this instance is for, decided by the people who
-- already administer it, so it joins the quorum in `instance_settings`.
ALTER TABLE instance_settings
    ADD COLUMN allow_registration BOOLEAN NOT NULL DEFAULT TRUE;

-- Each seeded setting remembers on its own whether an administrator has ever
-- saved it, rather than sharing the row-level `updated_at`.
--
-- A shared marker was fine while there was one setting. With two it goes wrong
-- on exactly the instances this migration lands on: an operator who set
-- ALLOW_REGISTRATION=false in .env and whose administrator has since saved a
-- quorum has a row that is already "configured" -- so a shared marker would
-- refuse to seed the new column, and the upgrade would silently reopen
-- sign-ups on a host that closed them. NULL here still means what
-- `updated_at IS NULL` meant: the environment may set it at boot, and stops
-- the moment an administrator saves a value.
ALTER TABLE instance_settings
    ADD COLUMN food_quorum_updated_at TIMESTAMPTZ,
    ADD COLUMN allow_registration_updated_at TIMESTAMPTZ;

-- The quorum's marker was the row's marker until now; carry it over so a
-- quorum an administrator already saved is not reseeded from FOOD_QUORUM on
-- the next boot.
UPDATE instance_settings SET food_quorum_updated_at = updated_at;
