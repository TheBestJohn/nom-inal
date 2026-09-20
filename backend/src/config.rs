use std::env;

/// Everything the process needs from the environment, resolved once at boot so
/// a missing variable fails fast instead of at the first request that needs it.
#[derive(Clone, Debug)]
pub struct Config {
    pub database_url: String,
    pub bind_addr: String,
    pub jwt_secret: String,
    pub jwt_ttl_hours: i64,
    pub cors_origins: Vec<String>,
    /// USDA FoodData Central key. Absent => USDA search is disabled, not fatal.
    pub usda_api_key: Option<String>,
    pub usda_base_url: String,
    pub off_base_url: String,
    /// Seeds whether sign-ups are open, the same way `initial_food_quorum`
    /// seeds the quorum: the live value is in `instance_settings`, an
    /// administrator changes it from the admin area, and this only says where
    /// a fresh install starts. `None` when the variable is unset, so a
    /// deployment that never mentions it leaves the database default alone.
    pub initial_allow_registration: Option<bool>,
    /// pg_trgm word-similarity threshold for the fuzzy search tier. Postgres
    /// defaults to 0.6, which is tuned for matching whole documents and is too
    /// strict for autocomplete: a one-letter typo in a short query lands around
    /// 0.5. Lower it and more typos match, at the cost of more noise.
    pub trgm_word_threshold: f64,
    /// Where photo files are written. A mounted volume in Docker.
    pub photo_dir: String,
    /// The address this instance is reached at from outside, e.g.
    /// `https://nom.example`. Only the shared-recipe page needs it, and only
    /// because Open Graph tags and canonical links have to be absolute: a
    /// server behind a proxy cannot learn its public name from its own
    /// socket. Unset means "take it from the request" — `X-Forwarded-Proto`
    /// and `Host`, which the bundled nginx sets — which is right for every
    /// ordinary deployment. Set it when the proxy in front does not forward
    /// them, or forwards a name that is not the public one.
    pub public_origin: Option<String>,
    /// Largest accepted upload, before downscaling.
    pub max_upload_bytes: usize,
    /// Seeds the verification quorum on an instance no administrator has
    /// configured yet. The live value lives in `instance_settings` and is
    /// changed from the admin area; this only decides where a fresh install
    /// starts, so a declarative deployment can still choose.
    pub initial_food_quorum: Option<i64>,
}

#[derive(Debug, thiserror::Error)]
#[error("missing required environment variable: {0}")]
pub struct MissingEnv(&'static str);

impl Config {
    pub fn from_env() -> Result<Self, MissingEnv> {
        Ok(Self {
            database_url: req("DATABASE_URL")?,
            bind_addr: opt("BIND_ADDR").unwrap_or_else(|| "0.0.0.0:8080".into()),
            jwt_secret: req("JWT_SECRET")?,
            jwt_ttl_hours: opt("JWT_TTL_HOURS")
                .and_then(|v| v.parse().ok())
                .unwrap_or(24 * 7),
            cors_origins: opt("CORS_ORIGINS")
                .map(|v| {
                    v.split(',')
                        .map(str::trim)
                        .filter(|s| !s.is_empty())
                        .map(String::from)
                        .collect()
                })
                .unwrap_or_default(),
            usda_api_key: opt("USDA_API_KEY").filter(|k| !k.is_empty()),
            usda_base_url: opt("USDA_BASE_URL")
                .unwrap_or_else(|| "https://api.nal.usda.gov/fdc/v1".into()),
            off_base_url: opt("OFF_BASE_URL")
                .unwrap_or_else(|| "https://world.openfoodfacts.org".into()),
            initial_allow_registration: opt("ALLOW_REGISTRATION").map(|v| v != "false" && v != "0"),
            photo_dir: opt("PHOTO_DIR").unwrap_or_else(|| "./data/photos".into()),
            public_origin: opt("PUBLIC_ORIGIN").map(|v| v.trim_end_matches('/').to_string()),
            max_upload_bytes: opt("MAX_UPLOAD_MB")
                .and_then(|v| v.parse::<usize>().ok())
                .filter(|mb| *mb > 0 && *mb <= 200)
                .unwrap_or(15)
                * 1024
                * 1024,
            initial_food_quorum: opt("FOOD_QUORUM")
                .and_then(|v| v.parse::<i64>().ok())
                .filter(|v| (1..=50).contains(v)),
            trgm_word_threshold: opt("TRGM_WORD_THRESHOLD")
                .and_then(|v| v.parse().ok())
                .filter(|v: &f64| (0.0..=1.0).contains(v))
                .unwrap_or(0.4),
        })
    }
}

fn req(key: &'static str) -> Result<String, MissingEnv> {
    env::var(key)
        .ok()
        .filter(|v| !v.is_empty())
        .ok_or(MissingEnv(key))
}

fn opt(key: &str) -> Option<String> {
    env::var(key).ok().filter(|v| !v.is_empty())
}
