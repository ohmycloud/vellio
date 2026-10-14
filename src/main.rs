#[cfg(feature = "desktop")]
mod desktop;
use anyhow::{Context, Result, bail};
use easyread_gpui::{
    ai, formula,
    model::ReaderState,
    store::{Library, Workspace},
};
use serde_json::{Value, json};
use std::{path::PathBuf, sync::atomic::AtomicBool};

fn main() -> Result<()> {
    let mut args = std::env::args().skip(1);
    let mut state_path = None;
    let mut library_path = None;
    let mut paper = None;
    let mut import = None;
    let mut migrate = None;
    let mut ops = None;
    let mut export = None;
    let mut ask = None;
    let mut translate = false;
    let mut translate_pages = None;
    let mut complete_images = false;
    let mut tid = "t-native".to_owned();
    let mut pdfium = None;
    let mut formula_tex = None;
    let mut formula_out = None;
    let mut headless = false;
    let mut list = false;
    while let Some(arg) = args.next() {
        let mut value = || {
            args.next()
                .with_context(|| format!("{arg} requires a value"))
        };
        match arg.as_str() {
            "--state" => state_path = Some(PathBuf::from(value()?)),
            "--library" => library_path = Some(PathBuf::from(value()?)),
            "--paper" => paper = Some(value()?),
            "--import" => import = Some(PathBuf::from(value()?)),
            "--migrate-copy" => migrate = Some(PathBuf::from(value()?)),
            "--ops" => ops = Some(PathBuf::from(value()?)),
            "--export" => export = Some(PathBuf::from(value()?)),
            "--ask" => ask = Some(value()?),
            "--thread" => tid = value()?,
            "--translate" => translate = true,
            "--translate-pages" => {
                translate_pages = Some(parse_pages(&value()?)?);
                translate = true;
            }
            "--complete-images" => complete_images = true,
            "--pdfium" => pdfium = Some(PathBuf::from(value()?)),
            "--formula" => formula_tex = Some(value()?),
            "--formula-out" => formula_out = Some(PathBuf::from(value()?)),
            "--check-state" | "--headless" => headless = true,
            "--list" => {
                list = true;
                headless = true;
            }
            "--help" | "-h" => {
                println!(
                    "Vellio Rust / GPUI\n  --library DIR [--paper ID] [--import PDF_OR_EPUB | --migrate-copy DIR]\n  --list | --headless | --check-state\n  --state STATE.json (snapshot mode)\n  --ops OPS.json --export STATE.json\n  --complete-images (with --paper, add missing PDF images)\n  --ask QUESTION [--thread ID] | --translate [--translate-pages 2,3,7] (requires EASYREAD_API_BASE/MODEL/API_KEY)\n  --formula TEX --formula-out OUTPUT.svg\n  --pdfium LIBRARY (or EASYREAD_PDFIUM_LIBRARY)\nNative window: cargo run --features desktop -- --library DIR"
                );
                return Ok(());
            }
            _ => bail!("Unknown argument: {arg}"),
        }
    }
    if let Some(tex) = formula_tex {
        let out = formula_out.context("--formula requires --formula-out")?;
        std::fs::write(&out, formula::svg(&tex, true)?)?;
        println!("Formula: {}", out.display());
        return Ok(());
    }
    if state_path.is_some()
        && (import.is_some()
            || migrate.is_some()
            || paper.is_some()
            || ops.is_some()
            || ask.is_some()
            || translate
            || complete_images)
    {
        bail!("Snapshot mode cannot mutate a library; choose --paper instead");
    }
    let library = if state_path.is_none() {
        Some(Library::open(library_path.unwrap_or_else(default_library))?)
    } else {
        None
    };
    let mut workspace: Option<Workspace> = None;
    if let Some(lib) = &library {
        if let Some(path) = import {
            workspace = Some(lib.import_file(&path, pdfium.as_deref())?);
        }
        if let Some(path) = migrate {
            workspace = Some(lib.migrate_copy(&path)?);
        }
        if let Some(id) = paper {
            workspace = Some(lib.workspace(&id)?);
        }
        if list {
            println!("{}", serde_json::to_string_pretty(&lib.list()?)?);
            return Ok(());
        }
    }
    if let Some(path) = ops {
        let operations: Vec<Value> = serde_json::from_slice(&std::fs::read(path)?)?;
        println!(
            "{}",
            workspace
                .as_ref()
                .context("--ops requires --paper or --import")?
                .reader_ops(&operations)?
        );
    }
    if complete_images {
        let count = workspace
            .as_ref()
            .context("--complete-images requires --paper")?
            .complete_pdf_images(&AtomicBool::new(false), |done, total| {
                if done % 25 == 0 || done == total {
                    eprintln!("Images {done}/{total}");
                }
            })?;
        println!("Added {count} image regions; existing text, edits and notes preserved");
        headless = true;
    }
    if let Some(q) = ask {
        let answer = ai::ask(
            workspace.as_ref().context("--ask requires --paper")?,
            &tid,
            &q,
            None,
            &ai::Config::environment()?,
            &AtomicBool::new(false),
            |_| {},
        )?;
        println!("{answer}");
        headless = true;
    }
    if translate {
        let ws = workspace.as_ref().context("--translate requires --paper")?;
        let config = ai::Config::environment()?;
        let options = ai::TranslationOptions::default();
        let cancel = AtomicBool::new(false);
        let progress = |done, total| eprintln!("Translation {done}/{total}");
        if let Some(pages) = translate_pages {
            ai::translate_pages_batched(ws, &config, options, &pages, &cancel, progress)?;
        } else {
            ai::translate_batched(ws, &config, options, &cancel, progress)?;
        }
        headless = true;
    }
    let raw = if let Some(path) = state_path {
        serde_json::from_slice(&std::fs::read(path)?)?
    } else if let Some(ws) = &workspace {
        ws.state()?
    } else {
        json!({"paper":{"meta":{"title_en":"Vellio"},"blocks":[]},"reader":{}})
    };
    let state = ReaderState::from_value(raw.clone())?;
    if let Some(path) = export {
        easyread_gpui::store::write_json(&path, &raw)?;
    }
    if headless {
        println!(
            "{}: {} blocks, {}, {} edits{}",
            state.title(),
            state.paper.blocks.len(),
            if raw["paper"]["meta"]["format"] == "epub" {
                format!(
                    "{} spine documents",
                    raw["paper"]["meta"]["chapter_count"].as_u64().unwrap_or(0)
                )
            } else {
                format!("{} pages", state.paper.meta.pages.len())
            },
            state.reader.edits.len(),
            workspace
                .as_ref()
                .map(|w| format!(" · {}", w.id()))
                .unwrap_or_default()
        );
        return Ok(());
    }
    #[cfg(feature = "desktop")]
    desktop::run(raw, library, workspace);
    #[cfg(not(feature = "desktop"))]
    println!("Use --features desktop for the native window, or --help for Rust CLI commands.");
    Ok(())
}
fn parse_pages(value: &str) -> Result<Vec<usize>> {
    value
        .split(',')
        .map(|page| {
            let page = page
                .trim()
                .parse::<usize>()
                .context("Use comma-separated positive page numbers, e.g. 2,3,7")?;
            anyhow::ensure!(page > 0, "Source PDF pages start at 1");
            Ok(page)
        })
        .collect()
}

fn default_library() -> PathBuf {
    if let Some(path) = std::env::var_os("EASYREAD_LIBRARY") {
        return path.into();
    }
    if let Some(path) = easyread_gpui::settings::saved_library() {
        return path;
    }
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));
    #[cfg(target_os = "macos")]
    return home.join("Library/Application Support/EasyReadGPUI/library");
    #[cfg(target_os = "windows")]
    return std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .unwrap_or(home)
        .join("EasyReadGPUI/library");
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    return home.join(".local/share/easyread-gpui/library");
}

#[cfg(test)]
mod argument_tests {
    use super::*;
    #[test]
    fn selected_pages_are_positive_comma_separated_numbers() {
        assert_eq!(parse_pages("2, 3,7").unwrap(), vec![2, 3, 7]);
        for invalid in ["", "0", "-1", "1,", "1-3", "two", "1,0"] {
            assert!(parse_pages(invalid).is_err(), "{invalid}");
        }
    }
}
