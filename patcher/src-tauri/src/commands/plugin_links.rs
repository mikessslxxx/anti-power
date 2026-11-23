//! Patch Anti-Power navigation links into a staged Usage Monitor release
use regex::Regex;
use serde::Deserialize;
use std::{fs, io, path::Path};

const ASSETS: [(&str, &str); 3] = [
    (
        "hooks.json",
        include_str!("../../usage-monitor-links/hooks.json"),
    ),
    (
        "panel.js",
        include_str!("../../usage-monitor-links/panel.js"),
    ),
    (
        "adapter.mjs",
        include_str!("../../usage-monitor-links/adapter.mjs"),
    ),
];

#[derive(Deserialize)]
struct Hook {
    path: String,
    before: Option<String>,
    after: Option<String>,
    code: String,
}

pub(super) fn apply(panel: &Path) -> Result<(), Box<dyn std::error::Error>> {
    for hook in serde_json::from_str::<Vec<Hook>>(ASSETS[0].1)? {
        let file = panel.join(&hook.path);
        let mut source = fs::read_to_string(&file)?;
        let pattern = Regex::new(
            hook.before
                .as_ref()
                .or(hook.after.as_ref())
                .ok_or("Missing link hook")?,
        )?;
        let mut matches = pattern.find_iter(&source);
        let insertion = matches
            .next()
            .filter(|_| matches.next().is_none())
            .ok_or_else(|| {
                io::Error::other(format!("Incompatible Anti-Power link hook: {}", hook.path))
            })?;
        let pos = if hook.after.is_some() {
            insertion.end()
        } else {
            insertion.start()
        };
        let code = if hook.code == "@panel" {
            ASSETS[1].1
        } else {
            &hook.code
        };
        source.insert_str(pos, &format!("\n{code}\n"));
        fs::write(file, source)?;
    }
    let assets = panel.join("anti-power");
    fs::create_dir(&assets)?;
    for (name, source) in ASSETS {
        fs::write(assets.join(name), source)?;
    }
    Ok(())
}
