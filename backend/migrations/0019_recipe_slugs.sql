-- A shared recipe's link says what it is, and old links do not rot.
--
-- Until now the only address a recipe had was its uuid, so every shared link
-- read `/r/3f7c…`: opaque to the person receiving it and worthless to a
-- search engine. A slug fixes that, and immediately raises the question every
-- slug scheme has to answer — what happens to the links already sent when the
-- recipe is renamed?
--
-- The answer here is that a slug is never retired. `recipe_slug_history` keeps
-- every slug a recipe has ever had, the current one included, and resolution
-- goes through it: an old slug still finds the recipe, and the page then
-- redirects to whatever it is called today. The slug is the primary key of
-- that table, so a slug freed by a rename can never be handed to a different
-- recipe — a stale link resolves to the dish it always meant, or to nothing.
--
-- Generating the slug is Rust's job (`domain::slug`), called from create and
-- from rename alike so there is one rule. The SQL below is the one exception:
-- existing rows need slugs before the column can be NOT NULL, and a backfill
-- cannot call into the application. It is written to agree with the Rust for
-- the names people actually have, and it is used once, here.

ALTER TABLE recipes ADD COLUMN slug TEXT;

CREATE TABLE recipe_slug_history (
    slug       TEXT PRIMARY KEY,
    recipe_id  UUID NOT NULL REFERENCES recipes(id) ON DELETE CASCADE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX recipe_slug_history_recipe_idx ON recipe_slug_history (recipe_id);

COMMENT ON TABLE recipe_slug_history IS
    'Every slug every recipe has ever had. The primary key is what stops a '
    'slug freed by a rename being reused by another recipe.';

-- ---------------------------------------------------------------------------
-- Backfill
-- ---------------------------------------------------------------------------

-- The base slug, before anything is known about collisions: the same rule as
-- `domain::slug::slugify`, written once here for the one-time backfill and
-- dropped at the end of this migration so there is no second copy left in the
-- database to drift from the Rust.
CREATE FUNCTION pg_temp.base_slug(name TEXT, id UUID) RETURNS TEXT AS $$
    WITH folded AS (
        SELECT translate(
            -- The multi-letter folds first: translate() maps character to
            -- character and would drop these rather than expand them.
            replace(replace(replace(replace(replace(replace(
                lower(name),
                'ß', 'ss'), 'æ', 'ae'), 'œ', 'oe'), 'þ', 'th'), 'ð', 'd'), 'ø', 'o'),
            'àáâãäåāăąçćĉċčďđèéêëēĕėęěĝğġģĥħìíîïĩīĭįıĵķĺļľŀłñńņňòóôõöōŏőŕŗřśŝşšţťŧùúûüũūŭůűųŵýÿŷźżž',
            'aaaaaaaaacccccddeeeeeeeeegggghhiiiiiiiiijklllllnnnnooooooooorrrsssstttuuuuuuuuuuwyyyzzz'
        ) AS name
    ), slugged AS (
        SELECT btrim(regexp_replace(name, '[^a-z0-9]+', '-', 'g'), '-') AS slug FROM folded
    ), capped AS (
        -- Cut on a word boundary where there is one, as the Rust does. A name
        -- whose first word alone is longer than the cap is simply cut.
        SELECT CASE
                   WHEN length(slug) <= 60 THEN slug
                   WHEN strpos(reverse(left(slug, 60)), '-') > 0
                       THEN left(slug, 60 - strpos(reverse(left(slug, 60)), '-'))
                   ELSE btrim(left(slug, 60), '-')
               END AS slug
          FROM slugged
    )
    -- A name with nothing sluggable in it — emoji, punctuation, a script we do
    -- not romanise — gets the short-id fallback the Rust uses.
    SELECT CASE WHEN slug = '' THEN 'recipe-' || left(replace(id::text, '-', ''), 8) ELSE slug END
      FROM capped;
$$ LANGUAGE sql IMMUTABLE;

-- Oldest first, each taking the first slug nobody before it took, which is
-- the order a fresh instance would have issued them in. A loop rather than a
-- window function because a suffixed slug can collide with a plain one — a
-- second "Pasta" wants `pasta-2`, which is exactly what a recipe named
-- "Pasta 2" is already called — and only checking each candidate as it is
-- assigned catches that.
DO $$
DECLARE
    row    RECORD;
    v_base TEXT;
    v_slug TEXT;
    v_n    INT;
BEGIN
    FOR row IN SELECT id, name FROM recipes ORDER BY created_at, id LOOP
        v_base := pg_temp.base_slug(row.name, row.id);
        v_slug := v_base;
        v_n := 1;
        WHILE EXISTS (SELECT 1 FROM recipes WHERE recipes.slug = v_slug) LOOP
            v_n := v_n + 1;
            v_slug := btrim(left(v_base, 60 - length(v_n::text) - 1), '-') || '-' || v_n;
        END LOOP;
        UPDATE recipes SET slug = v_slug WHERE recipes.id = row.id;
    END LOOP;
END
$$;

DROP FUNCTION pg_temp.base_slug(TEXT, UUID);

ALTER TABLE recipes
    ALTER COLUMN slug SET NOT NULL,
    ADD CONSTRAINT recipes_slug_key UNIQUE (slug);

INSERT INTO recipe_slug_history (slug, recipe_id)
SELECT slug, id FROM recipes;

-- The current slug is always in the history too. Deferred because a new
-- recipe is inserted before its history row inside the same transaction, and
-- because deleting a recipe removes the row that its own slug points at.
ALTER TABLE recipes
    ADD CONSTRAINT recipes_slug_in_history
        FOREIGN KEY (slug) REFERENCES recipe_slug_history(slug)
        DEFERRABLE INITIALLY DEFERRED;
