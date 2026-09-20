-- Why you are tracking.
--
-- The app stores the right things but has never known what they are for.
-- Someone counting sodium for their blood pressure and someone counting protein
-- for the gym were given the same four readouts, the same calorie chart and no
-- targets, and had to find their own way to Settings. The focus is the one
-- fact the rest of the setup can be derived from: which nutrients belong on
-- screen, which get charted, and which direction each target points.
--
-- It is a preset, not a policy. Choosing one writes ordinary targets and
-- display preferences that stay editable, and nothing anywhere reads the focus
-- to decide what a number means.
ALTER TABLE users
    ADD COLUMN tracking_focus TEXT
        CHECK (tracking_focus IN (
            'general', 'weight_loss', 'muscle_gain', 'keto',
            'diabetes', 'blood_pressure', 'heart_health', 'custom'
        ));

-- NULL means the question has never been asked, which is what sends an
-- account through the welcome flow once. 'custom' is the answer "none of
-- these" and is never asked again. Same distinction as `shown_nutrients`.
COMMENT ON COLUMN users.tracking_focus IS
    'What the account is tracking for. NULL = never chosen; custom = chose nothing.';

-- Net carbs (carbohydrate minus fibre) is a nutrient a target can be set on. It
-- is never stored: every food, recipe, entry and day derives it from the two
-- figures it already carries, so it cannot disagree with them. The vocabulary
-- check on targets has to learn the name, though, or a keto budget would be
-- refused by the constraint that exists to catch typos.
ALTER TABLE nutrition_targets
    DROP CONSTRAINT nutrition_targets_nutrient_known,
    ADD CONSTRAINT nutrition_targets_nutrient_known CHECK (nutrient IN (
        'calories_kcal', 'protein_g', 'carbs_g', 'net_carbs_g', 'fat_g',
        'fiber_g', 'sugar_g', 'saturated_fat_g', 'sodium_mg'
    ));
