-- What the person said they ate, kept beside what it weighed.
--
-- `food_portions` already answers "how many grams is one of these", and the
-- add-food dialog already uses it to fill the grams box. What was missing was
-- the count — "two chicken breasts", not "one" — and any memory of it. A
-- logged entry stored 348 g and nothing else, so the diary read "348 g" and
-- the sentence the person actually said was gone by the time they looked at
-- it. These two columns are that sentence.
--
-- The snapshot is a label and a count, deliberately not `portion_id`:
--
--   * Grams stay authoritative. Everything that adds up — the day's totals,
--     a recipe's macros, the estimators — reads `quantity_g` and goes on
--     reading it. The label is decoration on a number that already exists.
--   * A portion is a measure of the food, editable by anyone, and its weight
--     can be corrected. If a meal pointed at the row, correcting "1 breast"
--     from 174 g to 200 g would silently rewrite last Tuesday's lunch, and a
--     day that was logged honestly would change weight while nobody was
--     looking. The person ate 348 g. The label is what they said at the time;
--     the grams are what was counted, and neither is allowed to revise the
--     other afterwards.
--   * The same reason a portion can be deleted without taking a meal with it.
--     There is no foreign key here, so there is nothing to cascade.
--
-- Both columns or neither: a count with no label says nothing, and a label
-- with no count cannot be rendered. And only where there is a food to have
-- measures of — a recipe is logged in servings already, and a free-text
-- ingredient has no weight at all.

ALTER TABLE diary_entries
    ADD COLUMN portion_label TEXT,
    ADD COLUMN portion_count DOUBLE PRECISION CHECK (portion_count > 0);

ALTER TABLE diary_entries
    ADD CONSTRAINT diary_entry_portion_pair CHECK (
        (portion_label IS NULL AND portion_count IS NULL)
        OR (portion_label IS NOT NULL AND portion_count IS NOT NULL
            AND btrim(portion_label) <> '' AND food_id IS NOT NULL)
    );

COMMENT ON COLUMN diary_entries.portion_label IS
    'The household measure this amount was entered as, copied from the '
    'portion at the time. A snapshot, not a reference: correcting the '
    'portion later must not change what was already eaten.';

ALTER TABLE recipe_items
    ADD COLUMN portion_label TEXT,
    ADD COLUMN portion_count DOUBLE PRECISION CHECK (portion_count > 0);

ALTER TABLE recipe_items
    ADD CONSTRAINT recipe_item_portion_pair CHECK (
        (portion_label IS NULL AND portion_count IS NULL)
        OR (portion_label IS NOT NULL AND portion_count IS NOT NULL
            AND btrim(portion_label) <> '' AND food_id IS NOT NULL)
    );

COMMENT ON COLUMN recipe_items.portion_label IS
    'As on diary_entries: the measure an ingredient was written in, snapshotted '
    'beside the grams it came to. Only a food ingredient has one — a sub-recipe '
    'is taken in servings, and a free-text ingredient carries no quantity.';
