-- Household portions: "1 cup", "1 slice", "1 medium", each with its weight.
--
-- A diary entry stores grams, and keeps storing grams. Nobody weighs a cup of
-- rice, though; they measure a cup, and the question they actually have is
-- "how many grams is that for this food". A portion is that answer, kept per
-- food because a cup of oats and a cup of milk weigh nothing alike. USDA's
-- food detail publishes these as `foodPortions`; a person can add their own.
--
-- Portions live beside the food rather than in it. They are measures of the
-- food, not claims about its nutrition, so they sit outside the revision and
-- verification model: adding "1 mug · 300 g" does not bump the revision or
-- unsettle anyone's confirmation of the numbers, and a re-import refreshes the
-- provider's portions without touching the ones a person typed in.
CREATE TABLE food_portions (
    id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    food_id     UUID NOT NULL REFERENCES foods(id) ON DELETE CASCADE,
    label       TEXT NOT NULL CHECK (btrim(label) <> ''),
    grams       DOUBLE PRECISION NOT NULL CHECK (grams > 0),
    -- Who said so: 'usda' or 'off' for a provider's measure, 'user' for one
    -- typed in. A re-import replaces the provider's rows and never a person's.
    source      TEXT NOT NULL CHECK (source IN ('usda', 'off', 'user')),
    sort_order  INTEGER NOT NULL DEFAULT 0,
    -- Two "1 cup" rows with different weights would be a question, not data.
    UNIQUE (food_id, label)
);

CREATE INDEX food_portions_food_idx ON food_portions (food_id, sort_order);
