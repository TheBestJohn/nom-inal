-- How the home page plots the nutrients you follow.
--
-- 'percent' indexes every series to your goal or budget for that nutrient and
-- draws them on one chart. That is the honest way to get several nutrients onto
-- a single axis: calories and fat share no scale at all, but "how far through
-- today's number am I" is the same question for both, so 100% means the same
-- thing on every line.
--
-- 'actual' keeps the real figures, which cannot share an axis — a couple of
-- thousand kcal and seventy grams of fat on one scale flattens the fat line
-- onto the floor — so it draws one small chart per nutrient instead.
ALTER TABLE users
    ADD COLUMN chart_mode TEXT
        CHECK (chart_mode IN ('percent', 'actual'));

COMMENT ON COLUMN users.chart_mode IS
    'Home page chart style. NULL = follow the application default.';
