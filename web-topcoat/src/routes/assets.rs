//! Assets HTTP handlers.
use super::*;

#[route(GET "/math.svg")]
async fn serve_math_svg(
    cx: &Cx,
) -> Result<([(topcoat::router::HeaderName, &'static str); 1], Vec<u8>)> {
    let query = query_params::<MathQuery>(cx)?;
    if query.tex.len() > 4096 || query.tex.trim().is_empty() {
        return Err(bad_request("公式长度无效").into());
    }
    let tex = query.tex.clone();
    let display = query.display.unwrap_or(false);
    let svg = tokio::task::spawn_blocking(move || math_svg::render(&tex, display))
        .await?
        .map_err(Error::from_anyhow)?;
    Ok((
        [(topcoat::router::header::CONTENT_TYPE, "image/svg+xml")],
        svg.into_bytes(),
    ))
}

#[route(GET "/home.js")]
async fn home_script() -> Result<Js<&'static str>> {
    Ok(Js(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/assets/home.js"
    ))))
}
#[route(GET "/popover.css")]
async fn popover_styles() -> Result<Css<&'static str>> {
    Ok(Css(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/assets/popover.css"
    ))))
}
#[route(GET "/settings.css")]
async fn settings_styles() -> Result<Css<&'static str>> {
    Ok(Css(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/assets/settings.css"
    ))))
}
#[route(GET "/settings.js")]
async fn settings_script() -> Result<Js<&'static str>> {
    Ok(Js(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/assets/settings.js"
    ))))
}
#[route(GET "/reader.css")]
async fn styles() -> Result<Css<&'static str>> {
    Ok(Css(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/assets/reader.css"
    ))))
}
#[route(GET "/reader-updates.js")]
async fn reader_updates() -> Result<Js<&'static str>> {
    Ok(Js(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/assets/reader-updates.js"
    ))))
}
#[route(GET "/margin-editor.css")]
async fn margin_editor_css() -> Result<Css<&'static str>> {
    Ok(Css(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/assets/margin-editor.css"
    ))))
}
#[route(GET "/margin-editor.js")]
async fn margin_editor_js() -> Result<Js<&'static str>> {
    Ok(Js(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/assets/margin-editor.js"
    ))))
}
#[route(GET "/reader.js")]
async fn scripts() -> Result<Js<&'static str>> {
    Ok(Js(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/assets/reader.js"
    ))))
}
#[route(GET "/font/serif.ttf")]
async fn font() -> Result<([(topcoat::router::HeaderName, &'static str); 1], Vec<u8>)> {
    Ok((
        [(topcoat::router::header::CONTENT_TYPE, "font/ttf")],
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../assets/fonts/NotoSerifSC-Regular.ttf"
        ))
        .to_vec(),
    ))
}
#[route(GET "/read/{book_id}/{*asset_path}")]
async fn media(cx: &Cx) -> Result<([(topcoat::router::HeaderName, &'static str); 1], Vec<u8>)> {
    let id = path_param::<BookId>(cx);
    let segments = path_param::<AssetPath>(cx);
    let path = segments.collect::<Vec<_>>().join("/");
    let content_type = book_images::content_type(&path).ok_or_else(not_found)?;
    let workspace = library().workspace(id).map_err(|_| not_found())?;
    let file = workspace.asset(&path).map_err(|_| not_found())?;
    let bytes = tokio::fs::read(file).await.map_err(|_| not_found())?;
    Ok((
        [(topcoat::router::header::CONTENT_TYPE, content_type)],
        bytes,
    ))
}
