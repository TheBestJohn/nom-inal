-- What each person wants to see.
--
-- The readouts have always shown calories and the three macros, and the home
-- chart has always plotted calories alone. Both are reasonable defaults and
-- neither is right for everyone: someone tracking sodium for blood pressure, or
-- fibre on a doctor's advice, is watching a number the app never puts on screen.
--
-- Stored per account rather than per browser. The theme lives in localStorage
-- because it is genuinely device-dependent — dark on a phone at night, light on
-- a desktop at noon — but which nutrients you track is a fact about your diet,
-- and having fibre appear on the laptop and not the phone would just be a bug
-- with extra steps.
ALTER TABLE users
    ADD COLUMN shown_nutrients TEXT[],
    ADD COLUMN chart_nutrients TEXT[];

-- NULL means "never chosen", not "show nothing" — an empty array is how you say
-- that. Keeping the distinction means the application's default still reaches
-- everyone who has not expressed a preference, including when that default
-- changes later. Same reasoning as `instance_settings.updated_at`.
COMMENT ON COLUMN users.shown_nutrients IS
    'Nutrients shown in macro readouts. NULL = follow the application default.';
COMMENT ON COLUMN users.chart_nutrients IS
    'Nutrients plotted on the home page. NULL = follow the application default.';
