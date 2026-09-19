-- Photos on recipes, using the same storage as progress photos.
--
-- One table rather than a second copy of it. The bytes, the re-encoding, the
-- serving route and the delete-then-reclaim-the-file dance are identical for
-- both; the only thing that differs is what a photo is attached to and who may
-- see it. That is a column and a visibility rule, not a second table.
--
-- Visibility follows the subject: a weigh-in photo is its owner's alone, a
-- recipe photo is visible to whoever can see the recipe. That rule lives in
-- the serve route, which is the only place the bytes leave the server.
ALTER TABLE weigh_in_photos RENAME TO photos;
ALTER INDEX weigh_in_photos_entry_idx RENAME TO photos_entry_idx;
ALTER INDEX weigh_in_photos_user_idx  RENAME TO photos_user_idx;

ALTER TABLE photos
    ALTER COLUMN weight_entry_id DROP NOT NULL,
    ADD COLUMN recipe_id UUID REFERENCES recipes(id) ON DELETE CASCADE,
    -- Exactly one subject. The same shape as recipe_items: a photo of
    -- nothing, or of two things, is a bug the database refuses to store.
    ADD CONSTRAINT photos_one_subject
        CHECK ((weight_entry_id IS NULL) <> (recipe_id IS NULL));

CREATE INDEX photos_recipe_idx ON photos (recipe_id, created_at);
