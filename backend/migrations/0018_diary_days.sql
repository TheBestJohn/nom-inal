-- "I logged everything" per day.
--
-- An adaptive expenditure estimate is only as good as the intake it is fed,
-- and a diary cannot tell a fast day from a day someone stopped logging after
-- breakfast: both are a day with little in it. The flag is that distinction,
-- stated by the one person who knows. A day is complete when its owner says
-- so, and only complete days feed the estimator; every other day is treated
-- as missing data, never as a small number.
--
-- A row exists only once a day has been marked, so an unmarked day costs
-- nothing and a fast day is a row with `complete = true` and no entries —
-- a legitimate, informative zero, not an absence.
CREATE TABLE diary_days (
    user_id    UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    day        DATE NOT NULL,
    complete   BOOLEAN NOT NULL DEFAULT FALSE,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (user_id, day)
);

COMMENT ON TABLE diary_days IS
    'Per-day facts about a diary day. complete = the owner says every meal is logged.';
