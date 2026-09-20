-- Units are a display preference. Storage is metric and stays metric.
--
-- Body weight, target weight and height are stored in kilograms and
-- centimetres, as they always were; this column only says how to show them
-- and how to read what is typed. The client converts on the way in and out,
-- and the API never sees a pound.
--
-- Food amounts are deliberately not covered. A diary entry is grams whatever
-- the preference says, because "3 oz of chicken" is not how anyone measures
-- food at home — they measure a cup, a slice, a tablespoon — and those are
-- household portions, kept per food in `food_portions`, not a unit switch.
ALTER TABLE users
    ADD COLUMN units TEXT CHECK (units IN ('metric', 'imperial'));

-- NULL is metric: the default reaches everyone who never chose, and the
-- distinction between "never asked" and "chose metric" is not one anything
-- needs. Same reasoning as `chart_mode`.
COMMENT ON COLUMN users.units IS
    'How body measurements are shown and entered. NULL = metric.';
