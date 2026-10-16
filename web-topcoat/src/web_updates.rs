//! Local Web About / Updates. Shares the desktop's version/feed policy; the
//! browser can request a check but cannot supply an installer path or feed URL.
use super::*;
use easyread_gpui::updates;

struct State {
    cache: updates::Cache,
    checking: bool,
    error: String,
}
static STATE: OnceLock<Mutex<State>> = OnceLock::new();
fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}
fn state() -> &'static Mutex<State> {
    STATE.get_or_init(|| {
        let (cache, error) = match updates::Cache::load(&updates::state_path()) {
            Ok(cache) => (cache, String::new()),
            Err(error) => (
                updates::Cache {
                    enabled: false,
                    ..Default::default()
                },
                error.to_string(),
            ),
        };
        Mutex::new(State {
            cache,
            checking: false,
            error,
        })
    })
}
pub(super) fn snapshot() -> Value {
    let state = state().lock().unwrap_or_else(|p| p.into_inner());
    let latest = state
        .cache
        .release
        .as_ref()
        .filter(|r| r.is_newer_than(updates::VERSION));
    let download = latest.and_then(|r| r.download(std::env::consts::OS, std::env::consts::ARCH));
    json!({"current":updates::VERSION,"enabled":state.cache.enabled,"checking":state.checking,
        "status":if state.checking {"Checking…".into()}else{state.cache.status(updates::VERSION)},
        "error":state.error,"latest":latest.map(|r|r.version.as_str()).unwrap_or(""),
        "notes":latest.map(|r|r.notes.as_str()).unwrap_or(""),
        "download":download.map(|a|a.url.as_str()).unwrap_or(""),
        "platform":updates::platform_label(std::env::consts::OS,std::env::consts::ARCH)})
}
pub(super) fn check(force: bool) {
    let mut state = state().lock().unwrap_or_else(|p| p.into_inner());
    if state.checking || (!force && !state.cache.due(now())) {
        return;
    }
    state.checking = true;
    state.error.clear();
    drop(state);
    tokio::task::spawn_blocking(|| {
        let result = updates::fetch_latest();
        let mut state = self::state().lock().unwrap_or_else(|p| p.into_inner());
        state.checking = false;
        state.error = result
            .as_ref()
            .err()
            .map(ToString::to_string)
            .unwrap_or_default();
        state.cache.complete(&result, now());
        if let Err(error) = state.cache.save(&updates::state_path()) {
            state.error = error.to_string();
        }
    });
}
#[route(GET "/settings/updates/status")]
async fn status() -> Result<Json<Value>> {
    Ok(Json(snapshot()))
}
#[route(POST "/settings/updates/check")]
async fn check_now() -> Result<Json<Value>> {
    check(true);
    Ok(Json(snapshot()))
}
#[derive(Deserialize)]
struct Automatic {
    enabled: String,
}
#[route(POST "/settings/updates/automatic")]
async fn automatic(Form(input): Form<Automatic>) -> Result<Json<Value>> {
    if !matches!(input.enabled.as_str(), "true" | "false") {
        return Err(bad_request("Invalid preference").into());
    }
    {
        let mut state = state().lock().unwrap_or_else(|p| p.into_inner());
        let mut next = state.cache.clone();
        next.enabled = input.enabled == "true";
        next.save(&updates::state_path())
            .map_err(Error::from_anyhow)?;
        state.cache = next;
    }
    if input.enabled == "true" {
        check(false);
    }
    Ok(Json(snapshot()))
}
#[route(GET "/updates.js")]
async fn javascript() -> Result<Js<&'static str>> {
    Ok(Js(include_str!("../assets/updates.js")))
}
